//! x86-64 four-level paging descriptor primitives.
//!
//! This module encodes leaf entries and extracts page-table indices. It does
//! not allocate page tables, write CR3, modify active mappings, or invalidate
//! TLB entries. Callers must preserve the bootloader-provided mappings until
//! a complete replacement address space has been constructed and validated.

use crate::memory::{PhysAddr, VirtAddr};
use crate::pmm::{Pmm, PmmError};
use crate::vmm::{AddressGeometry, PageFlags, PageSize};

const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const WRITE_THROUGH: u64 = 1 << 3;
const CACHE_DISABLE: u64 = 1 << 4;
const HUGE_PAGE: u64 = 1 << 7;
const GLOBAL: u64 = 1 << 8;
const NO_EXECUTE: u64 = 1 << 63;
const PAGE_OFFSET_MASK: u64 = 0xfff;

/// Errors returned while encoding x86-64 paging structures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum X86PagingError {
    InvalidPhysicalAddressWidth,
    PhysicalAddressOutOfRange,
    MisalignedPhysicalAddress,
    NoExecuteUnavailable,
    InvalidAddressGeometry,
    NonCanonicalAddress,
    InvalidRootTable,
    MisalignedVirtualAddress,
    MissingIntermediateTable,
    InvalidHierarchy,
    AlreadyMapped,
    NotMapped,
    InsufficientIntermediatePermissions,
    InsufficientIntermediateExecutePermission,
    MemoryAccessFailed,
}

/// A raw x86-64 paging entry.
///
/// The value is private so callers cannot accidentally create a present entry
/// without passing through the validation routines in this module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// Encode a 4 KiB, 2 MiB, or 1 GiB leaf mapping.
    ///
    /// The physical address width must match the CPU. Non-executable mappings
    /// require EFER.NXE to be enabled before this entry is installed live.
    pub fn encode_leaf(
        physical_base: PhysAddr,
        page_size: PageSize,
        flags: PageFlags,
        physical_address_bits: u8,
        nx_enabled: bool,
    ) -> Result<Self, X86PagingError> {
        if !(32..=52).contains(&physical_address_bits) {
            return Err(X86PagingError::InvalidPhysicalAddressWidth);
        }

        let size = page_size.bytes().get();
        if physical_base.get() % size != 0 {
            return Err(X86PagingError::MisalignedPhysicalAddress);
        }

        let physical_limit = 1u64 << physical_address_bits;
        let physical_end = physical_base
            .get()
            .checked_add(size)
            .ok_or(X86PagingError::PhysicalAddressOutOfRange)?;
        if physical_base.get() >= physical_limit || physical_end > physical_limit {
            return Err(X86PagingError::PhysicalAddressOutOfRange);
        }

        let mut raw = physical_base.get() | PRESENT;
        if flags.bits() & PageFlags::WRITABLE != 0 {
            raw |= WRITABLE;
        }
        if flags.bits() & PageFlags::USER != 0 {
            raw |= USER;
        }
        if flags.bits() & PageFlags::WRITE_THROUGH != 0 {
            raw |= WRITE_THROUGH;
        }
        if flags.bits() & PageFlags::CACHE_DISABLE != 0 {
            raw |= CACHE_DISABLE;
        }
        if flags.bits() & PageFlags::GLOBAL != 0 {
            raw |= GLOBAL;
        }

        if !flags.is_executable() {
            if !nx_enabled {
                return Err(X86PagingError::NoExecuteUnavailable);
            }
            raw |= NO_EXECUTE;
        }

        if !matches!(page_size, PageSize::Size4KiB) {
            raw |= HUGE_PAGE;
        }

        Ok(Self(raw))
    }

    /// Return the encoded hardware entry value.
    pub const fn raw(self) -> u64 {
        self.0
    }

    pub const fn is_present(self) -> bool {
        self.0 & PRESENT != 0
    }

    pub const fn is_writable(self) -> bool {
        self.0 & WRITABLE != 0
    }

    pub const fn is_user(self) -> bool {
        self.0 & USER != 0
    }

    pub const fn is_global(self) -> bool {
        self.0 & GLOBAL != 0
    }

    pub const fn is_no_execute(self) -> bool {
        self.0 & NO_EXECUTE != 0
    }

    pub const fn is_huge_page(self) -> bool {
        self.0 & HUGE_PAGE != 0
    }

    /// Physical base encoded in this leaf entry.
    pub const fn physical_base(self) -> PhysAddr {
        PhysAddr::new(self.0 & !PAGE_OFFSET_MASK & !(1u64 << 63))
    }
}

/// Four-level x86-64 page-table indices (9 bits per level).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageTableIndices {
    pub pml4: u16,
    pub pdpt: u16,
    pub pd: u16,
    pub pt: u16,
}

impl PageTableIndices {
    /// Derive indices after validating a 48-bit canonical virtual address.
    pub fn from_address(
        address: VirtAddr,
        geometry: AddressGeometry,
    ) -> Result<Self, X86PagingError> {
        if geometry.virtual_address_bits() != 48 {
            return Err(X86PagingError::InvalidAddressGeometry);
        }
        if !geometry.is_canonical(address) {
            return Err(X86PagingError::NonCanonicalAddress);
        }
        let value = address.get();
        Ok(Self {
            pml4: ((value >> 39) & 0x1ff) as u16,
            pdpt: ((value >> 30) & 0x1ff) as u16,
            pd: ((value >> 21) & 0x1ff) as u16,
            pt: ((value >> 12) & 0x1ff) as u16,
        })
    }
}

/// Abstract access to page-table memory.
///
/// Implementations must only expose mapped, writable page-table storage.
/// This interface intentionally does not activate the root or issue TLB
/// invalidations; it is suitable for synthetic tests and a future HHDM-backed
/// implementation.
pub trait PageTableMemory {
    /// Read one 64-bit entry from a 4 KiB table (512 entries).
    fn read_entry(&self, table: PhysAddr, index: u16) -> Option<u64>;

    /// Write one 64-bit entry. Return false if the table is not accessible.
    fn write_entry(&mut self, table: PhysAddr, index: u16, value: u64) -> bool;
}

/// Direct page-table memory access through the active Limine HHDM.
///
/// This adapter only translates physical table addresses to their HHDM
/// aliases. It does not allocate tables, establish mappings, serialize
/// concurrent writers, or perform TLB invalidation.
pub struct HhdmPageTableMemory {
    hhdm_offset: u64,
    physical_address_bits: u8,
}

impl HhdmPageTableMemory {
    /// Create an HHDM-backed page-table accessor.
    ///
    /// # Safety
    /// The caller must guarantee that the HHDM is active and remains active
    /// for the lifetime of this value, and that every physical 4 KiB table
    /// page passed to its access methods is mapped writable through
    /// `hhdm_offset + physical_address`. The caller must also ensure that
    /// table pages remain allocated and are not concurrently mutated except
    /// under an external page-table synchronization protocol.
    pub unsafe fn new(hhdm_offset: u64, physical_address_bits: u8) -> Result<Self, X86PagingError> {
        if !(32..=52).contains(&physical_address_bits) {
            return Err(X86PagingError::InvalidPhysicalAddressWidth);
        }
        Ok(Self {
            hhdm_offset,
            physical_address_bits,
        })
    }

    fn entry_pointer(&self, table: PhysAddr, index: u16) -> Option<*mut u64> {
        if index >= 512 || table.get() & PAGE_OFFSET_MASK != 0 {
            return None;
        }

        let physical_limit = 1u64.checked_shl(self.physical_address_bits as u32)?;
        let physical_end = table.get().checked_add(4096)?;
        if physical_end > physical_limit {
            return None;
        }

        let virtual_base = self.hhdm_offset.checked_add(table.get())?;
        let virtual_last = virtual_base.checked_add(4095)?;
        let geometry = AddressGeometry::new(48).ok()?;
        if !geometry.is_canonical(VirtAddr::new(virtual_base))
            || !geometry.is_canonical(VirtAddr::new(virtual_last))
        {
            return None;
        }

        let address = usize::try_from(virtual_base).ok()?;
        address.checked_add(4095)?;
        Some((address as *mut u64).wrapping_add(index as usize))
    }
}

impl PageTableMemory for HhdmPageTableMemory {
    fn read_entry(&self, table: PhysAddr, index: u16) -> Option<u64> {
        let pointer = self.entry_pointer(table, index)?;
        // SAFETY: Construction is unsafe and its contract guarantees that
        // every accessed table page is mapped and readable through the HHDM.
        Some(unsafe { core::ptr::read_volatile(pointer) })
    }

    fn write_entry(&mut self, table: PhysAddr, index: u16, value: u64) -> bool {
        let Some(pointer) = self.entry_pointer(table, index) else {
            return false;
        };
        // SAFETY: Construction is unsafe and its contract guarantees that
        // every accessed table page is mapped writable through the HHDM.
        unsafe { core::ptr::write_volatile(pointer, value) };
        true
    }
}

/// Errors while obtaining and managing PMM-backed page-table frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageTablePoolError {
    CapacityExhausted,
    InvalidHhdmAddress,
    FrameNotOwned,
    Pmm(PmmError),
}

/// Errors while constructing a page-table hierarchy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageTableHierarchyError {
    Paging(X86PagingError),
    Pool(PageTablePoolError),
}

/// Owns a bounded set of zero-initialized page-table frames from the PMM.
///
/// The pool deliberately does not create parent entries or activate a root.
/// Its fixed capacity avoids requiring a kernel heap during early boot.
pub struct PageTableFramePool<'p, 'r, const CAPACITY: usize> {
    pmm: &'p mut Pmm<'r>,
    memory: HhdmPageTableMemory,
    owned: [Option<PhysAddr>; CAPACITY],
    owned_count: usize,
}

impl<'p, 'r, const CAPACITY: usize> PageTableFramePool<'p, 'r, CAPACITY> {
    /// Construct a page-table frame pool over the existing bootstrap PMM.
    ///
    /// # Safety
    /// The HHDM must remain active and writable for the pool lifetime. Every
    /// frame returned by the PMM must be mapped by the HHDM. The caller must
    /// serialize all PMM and page-table operations on this single CPU.
    pub unsafe fn new(
        pmm: &'p mut Pmm<'r>,
        hhdm_offset: u64,
        physical_address_bits: u8,
    ) -> Result<Self, X86PagingError> {
        // SAFETY: This constructor forwards the caller's HHDM mapping contract.
        let memory = unsafe { HhdmPageTableMemory::new(hhdm_offset, physical_address_bits)? };
        Ok(Self {
            pmm,
            memory,
            owned: [None; CAPACITY],
            owned_count: 0,
        })
    }

    /// Allocate one PMM frame and clear all 4096 bytes before exposing it.
    pub fn allocate_zeroed_table(&mut self) -> Result<PhysAddr, PageTablePoolError> {
        if self.owned_count == CAPACITY {
            return Err(PageTablePoolError::CapacityExhausted);
        }

        let frame = self
            .pmm
            .allocate_frame(0x5749_434f_4645_5054)
            .map_err(PageTablePoolError::Pmm)?;
        let Some(pointer) = self.memory.entry_pointer(frame, 0) else {
            let _ = self.pmm.free_frame(frame);
            return Err(PageTablePoolError::InvalidHhdmAddress);
        };

        // SAFETY: The pool constructor requires the HHDM to map every PMM
        // frame writable. The frame was just allocated and is exclusively
        // owned by this pool, so clearing its full page is valid.
        unsafe { core::ptr::write_bytes(pointer.cast::<u8>(), 0, 4096) };

        let Some(slot) = self.owned.iter_mut().find(|slot| slot.is_none()) else {
            let _ = self.pmm.free_frame(frame);
            return Err(PageTablePoolError::CapacityExhausted);
        };
        *slot = Some(frame);
        self.owned_count += 1;
        Ok(frame)
    }

    /// Release a frame previously allocated by this pool.
    ///
    /// # Safety
    /// The caller must first remove every page-table reference to the frame,
    /// ensure no CPU can walk or access it, and complete the required TLB
    /// shootdown before releasing it. This method does not edit parent entries
    /// or invalidate TLBs.
    pub unsafe fn release_table_frame(
        &mut self,
        frame: PhysAddr,
    ) -> Result<(), PageTablePoolError> {
        let Some(slot) = self.owned.iter_mut().find(|slot| **slot == Some(frame)) else {
            return Err(PageTablePoolError::FrameNotOwned);
        };
        let Some(pointer) = self.memory.entry_pointer(frame, 0) else {
            return Err(PageTablePoolError::InvalidHhdmAddress);
        };

        // SAFETY: The caller guarantees that no page-table walker can still
        // access this detached frame. The HHDM mapping remains valid.
        unsafe { core::ptr::write_bytes(pointer.cast::<u8>(), 0, 4096) };
        self.pmm
            .free_frame(frame)
            .map_err(PageTablePoolError::Pmm)?;
        *slot = None;
        self.owned_count -= 1;
        Ok(())
    }

    /// Access the HHDM-backed page-table memory adapter.
    pub fn memory(&mut self) -> &mut HhdmPageTableMemory {
        &mut self.memory
    }

    /// Number of page-table frames currently owned by this pool.
    pub const fn owned_frame_count(&self) -> usize {
        self.owned_count
    }

    /// Create missing intermediate tables and install one leaf mapping.
    ///
    /// On failure, newly created parent entries are detached and their frames
    /// are returned to the PMM. Existing hierarchy entries are left untouched.
    ///
    /// # Safety
    /// The root hierarchy must be inactive and inaccessible to every CPU for
    /// the duration of this operation. The caller must not publish the root
    /// until construction is complete. This avoids requiring TLB invalidation
    /// during rollback.
    pub unsafe fn map_leaf_with_tables(
        &mut self,
        root: PhysAddr,
        virtual_address: VirtAddr,
        physical_base: PhysAddr,
        page_size: PageSize,
        flags: PageFlags,
        physical_address_bits: u8,
        nx_enabled: bool,
    ) -> Result<(), PageTableHierarchyError> {
        let mask = physical_address_mask(physical_address_bits)
            .map_err(PageTableHierarchyError::Paging)?;
        if root.get() & PAGE_OFFSET_MASK != 0 || root.get() & !mask != 0 {
            return Err(PageTableHierarchyError::Paging(
                X86PagingError::InvalidRootTable,
            ));
        }
        let size = page_size.bytes().get();
        if virtual_address.get() % size != 0 {
            return Err(PageTableHierarchyError::Paging(
                X86PagingError::MisalignedVirtualAddress,
            ));
        }
        PageTableEntry::encode_leaf(
            physical_base,
            page_size,
            flags,
            physical_address_bits,
            nx_enabled,
        )
        .map_err(PageTableHierarchyError::Paging)?;
        let geometry = AddressGeometry::new(48)
            .map_err(|_| PageTableHierarchyError::Paging(X86PagingError::InvalidAddressGeometry))?;
        let indices = PageTableIndices::from_address(virtual_address, geometry)
            .map_err(PageTableHierarchyError::Paging)?;
        let mut created: [Option<(PhysAddr, u16, PhysAddr)>; 3] = [None; 3];
        let mut created_count = 0usize;

        let result = (|| {
            let pdpt = self.ensure_child_table(
                root,
                indices.pml4,
                flags,
                physical_address_bits,
                &mut created,
                &mut created_count,
            )?;
            if !matches!(page_size, PageSize::Size1GiB) {
                let pd = self.ensure_child_table(
                    pdpt,
                    indices.pdpt,
                    flags,
                    physical_address_bits,
                    &mut created,
                    &mut created_count,
                )?;
                if matches!(page_size, PageSize::Size4KiB) {
                    self.ensure_child_table(
                        pd,
                        indices.pd,
                        flags,
                        physical_address_bits,
                        &mut created,
                        &mut created_count,
                    )?;
                }
            }
            map_leaf(
                &mut self.memory,
                root,
                virtual_address,
                physical_base,
                page_size,
                flags,
                physical_address_bits,
                nx_enabled,
            )
            .map_err(PageTableHierarchyError::Paging)
        })();

        if result.is_err() {
            while created_count > 0 {
                created_count -= 1;
                if let Some((parent, index, child)) = created[created_count] {
                    if self.memory.write_entry(parent, index, 0) {
                        // SAFETY: The root is required to be inactive and the
                        // child has just been detached by clearing its parent.
                        let _ = unsafe { self.release_table_frame(child) };
                    }
                }
            }
        }
        result
    }

    /// Remove a leaf mapping and reclaim empty intermediate tables owned by this pool.
    ///
    /// Only page-table frames allocated by this pool are reclaimed. The root
    /// table is never reclaimed by this operation.
    ///
    /// # Safety
    /// The address-space root must be inactive and inaccessible to every CPU.
    /// The caller must ensure that no page-table walker can race this update
    /// and that no TLB entry can reference the removed mapping or reclaimed
    /// tables. This method does not invalidate TLBs or free the mapped data
    /// frame.
    pub unsafe fn unmap_leaf_with_reclaim(
        &mut self,
        root: PhysAddr,
        virtual_address: VirtAddr,
        page_size: PageSize,
        physical_address_bits: u8,
    ) -> Result<(), PageTableHierarchyError> {
        let geometry = AddressGeometry::new(48)
            .map_err(|_| PageTableHierarchyError::Paging(X86PagingError::InvalidAddressGeometry))?;
        let indices = PageTableIndices::from_address(virtual_address, geometry)
            .map_err(PageTableHierarchyError::Paging)?;
        let mask = physical_address_mask(physical_address_bits)
            .map_err(PageTableHierarchyError::Paging)?;
        let root_entry = read_table_entry(&self.memory, root, indices.pml4)
            .map_err(PageTableHierarchyError::Paging)?;
        if root_entry & PRESENT == 0 || root_entry & HUGE_PAGE != 0 {
            return Err(PageTableHierarchyError::Paging(X86PagingError::MissingIntermediateTable));
        }
        let pdpt = PhysAddr::new(root_entry & mask);
        let mut reclaim: [Option<(PhysAddr, u16, PhysAddr)>; 3] = [None; 3];
        let mut count = 0usize;

        if matches!(page_size, PageSize::Size1GiB) {
            reclaim[count] = Some((root, indices.pml4, pdpt));
            count += 1;
        } else {
            let pdpt_entry = read_table_entry(&self.memory, pdpt, indices.pdpt)
                .map_err(PageTableHierarchyError::Paging)?;
            if pdpt_entry & PRESENT == 0 || pdpt_entry & HUGE_PAGE != 0 {
                return Err(PageTableHierarchyError::Paging(X86PagingError::MissingIntermediateTable));
            }
            let pd = PhysAddr::new(pdpt_entry & mask);
            if matches!(page_size, PageSize::Size2MiB) {
                reclaim[count] = Some((pdpt, indices.pdpt, pd));
                count += 1;
            } else {
                let pd_entry = read_table_entry(&self.memory, pd, indices.pd)
                    .map_err(PageTableHierarchyError::Paging)?;
                if pd_entry & PRESENT == 0 || pd_entry & HUGE_PAGE != 0 {
                    return Err(PageTableHierarchyError::Paging(X86PagingError::MissingIntermediateTable));
                }
                let pt = PhysAddr::new(pd_entry & mask);
                reclaim[count] = Some((pd, indices.pd, pt));
                count += 1;
                reclaim[count] = Some((pdpt, indices.pdpt, pd));
                count += 1;
            }
            reclaim[count] = Some((root, indices.pml4, pdpt));
            count += 1;
        }

        unmap_leaf(
            &mut self.memory,
            root,
            virtual_address,
            page_size,
            physical_address_bits,
        )
        .map_err(PageTableHierarchyError::Paging)?;

        for position in reclaim.iter().take(count).flatten() {
            let (parent, index, child) = *position;
            self.reclaim_empty_child(parent, index, child)?;
        }
        Ok(())
    }

    fn reclaim_empty_child(
        &mut self,
        parent: PhysAddr,
        index: u16,
        child: PhysAddr,
    ) -> Result<(), PageTableHierarchyError> {
        if !self.owned.iter().any(|slot| *slot == Some(child)) {
            return Ok(());
        }
        for entry_index in 0..512 {
            let entry = self.memory.read_entry(child, entry_index)
                .ok_or(PageTableHierarchyError::Paging(X86PagingError::MemoryAccessFailed))?;
            if entry & PRESENT != 0 {
                return Ok(());
            }
        }
        if !self.memory.write_entry(parent, index, 0) {
            return Err(PageTableHierarchyError::Paging(X86PagingError::MemoryAccessFailed));
        }
        // SAFETY: The caller of unmap_leaf_with_reclaim guarantees the
        // hierarchy is inactive; this child is now detached and empty.
        unsafe { self.release_table_frame(child) }.map_err(PageTableHierarchyError::Pool)
    }

    fn ensure_child_table(
        &mut self,
        parent: PhysAddr,
        index: u16,
        flags: PageFlags,
        physical_address_bits: u8,
        created: &mut [Option<(PhysAddr, u16, PhysAddr)>; 3],
        created_count: &mut usize,
    ) -> Result<PhysAddr, PageTableHierarchyError> {
        let current =
            self.memory
                .read_entry(parent, index)
                .ok_or(PageTableHierarchyError::Paging(
                    X86PagingError::MemoryAccessFailed,
                ))?;
        if current & PRESENT != 0 {
            if current & HUGE_PAGE != 0 {
                return Err(PageTableHierarchyError::Paging(
                    X86PagingError::InvalidHierarchy,
                ));
            }
            let mask = physical_address_mask(physical_address_bits)
                .map_err(PageTableHierarchyError::Paging)?;
            check_parent_permissions(current, flags).map_err(PageTableHierarchyError::Paging)?;
            return Ok(PhysAddr::new(current & mask));
        }

        let child = self
            .allocate_zeroed_table()
            .map_err(PageTableHierarchyError::Pool)?;
        let mut entry = child.get() | PRESENT | WRITABLE;
        if flags.is_user() {
            entry |= USER;
        }
        if !self.memory.write_entry(parent, index, entry) {
            // SAFETY: The child was allocated but never linked into a hierarchy.
            unsafe { self.release_table_frame(child) }.map_err(PageTableHierarchyError::Pool)?;
            return Err(PageTableHierarchyError::Paging(
                X86PagingError::MemoryAccessFailed,
            ));
        }
        created[*created_count] = Some((parent, index, child));
        *created_count += 1;
        Ok(child)
    }
}

/// Result of translating a virtual address through a four-level hierarchy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Translation {
    pub physical_address: PhysAddr,
    pub page_size: PageSize,
}

fn physical_address_mask(physical_address_bits: u8) -> Result<u64, X86PagingError> {
    if !(32..=52).contains(&physical_address_bits) {
        return Err(X86PagingError::InvalidPhysicalAddressWidth);
    }
    Ok(((1u64 << physical_address_bits) - 1) & !PAGE_OFFSET_MASK)
}

fn read_table_entry<M: PageTableMemory>(
    memory: &M,
    table: PhysAddr,
    index: u16,
) -> Result<u64, X86PagingError> {
    memory
        .read_entry(table, index)
        .ok_or(X86PagingError::MemoryAccessFailed)
}

fn leaf_slot<M: PageTableMemory>(
    memory: &M,
    root: PhysAddr,
    indices: PageTableIndices,
    page_size: PageSize,
    physical_address_bits: u8,
) -> Result<(PhysAddr, u16), X86PagingError> {
    let mask = physical_address_mask(physical_address_bits)?;
    if root.get() & PAGE_OFFSET_MASK != 0 || root.get() & !mask != 0 {
        return Err(X86PagingError::InvalidRootTable);
    }

    let pml4e = read_table_entry(memory, root, indices.pml4)?;
    if pml4e & PRESENT == 0 {
        return Err(X86PagingError::MissingIntermediateTable);
    }
    if pml4e & HUGE_PAGE != 0 {
        return Err(X86PagingError::InvalidHierarchy);
    }
    let pdpt = PhysAddr::new(pml4e & mask);

    if matches!(page_size, PageSize::Size1GiB) {
        return Ok((pdpt, indices.pdpt));
    }

    let pdpte = read_table_entry(memory, pdpt, indices.pdpt)?;
    if pdpte & PRESENT == 0 {
        return Err(X86PagingError::MissingIntermediateTable);
    }
    if matches!(page_size, PageSize::Size1GiB) || pdpte & HUGE_PAGE != 0 {
        return Err(X86PagingError::InvalidHierarchy);
    }
    let pd = PhysAddr::new(pdpte & mask);

    if matches!(page_size, PageSize::Size2MiB) {
        return Ok((pd, indices.pd));
    }

    let pde = read_table_entry(memory, pd, indices.pd)?;
    if pde & PRESENT == 0 {
        return Err(X86PagingError::MissingIntermediateTable);
    }
    if pde & HUGE_PAGE != 0 {
        return Err(X86PagingError::InvalidHierarchy);
    }
    let pt = PhysAddr::new(pde & mask);
    Ok((pt, indices.pt))
}

fn validate_parent_permissions<M: PageTableMemory>(
    memory: &M,
    root: PhysAddr,
    indices: PageTableIndices,
    page_size: PageSize,
    physical_address_bits: u8,
    flags: PageFlags,
) -> Result<(), X86PagingError> {
    let mask = physical_address_mask(physical_address_bits)?;
    if root.get() & PAGE_OFFSET_MASK != 0 || root.get() & !mask != 0 {
        return Err(X86PagingError::InvalidRootTable);
    }

    let pml4e = read_table_entry(memory, root, indices.pml4)?;
    if pml4e & PRESENT == 0 || pml4e & HUGE_PAGE != 0 {
        return Err(X86PagingError::InvalidHierarchy);
    }
    check_parent_permissions(pml4e, flags)?;

    if matches!(page_size, PageSize::Size1GiB) {
        return Ok(());
    }

    let pdpt = PhysAddr::new(pml4e & mask);
    let pdpte = read_table_entry(memory, pdpt, indices.pdpt)?;
    if pdpte & PRESENT == 0 || pdpte & HUGE_PAGE != 0 {
        return Err(X86PagingError::InvalidHierarchy);
    }
    check_parent_permissions(pdpte, flags)?;

    if matches!(page_size, PageSize::Size2MiB) {
        return Ok(());
    }

    let pd = PhysAddr::new(pdpte & mask);
    let pde = read_table_entry(memory, pd, indices.pd)?;
    if pde & PRESENT == 0 || pde & HUGE_PAGE != 0 {
        return Err(X86PagingError::InvalidHierarchy);
    }
    check_parent_permissions(pde, flags)
}

fn check_parent_permissions(entry: u64, flags: PageFlags) -> Result<(), X86PagingError> {
    let missing_write_permission = flags.is_writable() && entry & WRITABLE == 0;
    let missing_user_permission = flags.is_user() && entry & USER == 0;
    if missing_write_permission || missing_user_permission {
        return Err(X86PagingError::InsufficientIntermediatePermissions);
    }
    if flags.is_executable() && entry & NO_EXECUTE != 0 {
        return Err(X86PagingError::InsufficientIntermediateExecutePermission);
    }
    Ok(())
}

/// Install a leaf mapping in an already populated four-level hierarchy.
///
/// Intermediate tables are never allocated implicitly. This keeps PMM
/// ownership and rollback outside this primitive until those contracts are
/// implemented. The operation does not invalidate TLB entries.
pub fn map_leaf<M: PageTableMemory>(
    memory: &mut M,
    root: PhysAddr,
    virtual_address: VirtAddr,
    physical_base: PhysAddr,
    page_size: PageSize,
    flags: PageFlags,
    physical_address_bits: u8,
    nx_enabled: bool,
) -> Result<(), X86PagingError> {
    let geometry = AddressGeometry::new(48).map_err(|_| X86PagingError::InvalidAddressGeometry)?;
    let size = page_size.bytes().get();
    if virtual_address.get() % size != 0 {
        return Err(X86PagingError::MisalignedVirtualAddress);
    }
    let indices = PageTableIndices::from_address(virtual_address, geometry)?;
    let encoded = PageTableEntry::encode_leaf(
        physical_base,
        page_size,
        flags,
        physical_address_bits,
        nx_enabled,
    )?;
    let (table, index) = leaf_slot(memory, root, indices, page_size, physical_address_bits)?;
    let current = read_table_entry(memory, table, index)?;
    if current & PRESENT != 0 {
        return Err(X86PagingError::AlreadyMapped);
    }
    validate_parent_permissions(
        memory,
        root,
        indices,
        page_size,
        physical_address_bits,
        flags,
    )?;
    if !memory.write_entry(table, index, encoded.raw()) {
        return Err(X86PagingError::MemoryAccessFailed);
    }
    Ok(())
}

/// Remove a leaf mapping from an existing hierarchy.
///
/// Empty intermediate tables are retained. Reclaiming them requires explicit
/// PMM ownership tracking and is intentionally left for a later milestone.
pub fn unmap_leaf<M: PageTableMemory>(
    memory: &mut M,
    root: PhysAddr,
    virtual_address: VirtAddr,
    page_size: PageSize,
    physical_address_bits: u8,
) -> Result<(), X86PagingError> {
    let geometry = AddressGeometry::new(48).map_err(|_| X86PagingError::InvalidAddressGeometry)?;
    let size = page_size.bytes().get();
    if virtual_address.get() % size != 0 {
        return Err(X86PagingError::MisalignedVirtualAddress);
    }
    let indices = PageTableIndices::from_address(virtual_address, geometry)?;
    let (table, index) = leaf_slot(memory, root, indices, page_size, physical_address_bits)?;
    let current = read_table_entry(memory, table, index)?;
    if current & PRESENT == 0 {
        return Err(X86PagingError::NotMapped);
    }
    let expects_huge = !matches!(page_size, PageSize::Size4KiB);
    if (current & HUGE_PAGE != 0) != expects_huge {
        return Err(X86PagingError::InvalidHierarchy);
    }
    if !memory.write_entry(table, index, 0) {
        return Err(X86PagingError::MemoryAccessFailed);
    }
    Ok(())
}

/// Translate a canonical virtual address using 4 KiB or huge-page leaves.
pub fn translate<M: PageTableMemory>(
    memory: &M,
    root: PhysAddr,
    virtual_address: VirtAddr,
    physical_address_bits: u8,
) -> Result<Translation, X86PagingError> {
    let geometry = AddressGeometry::new(48).map_err(|_| X86PagingError::InvalidAddressGeometry)?;
    let indices = PageTableIndices::from_address(virtual_address, geometry)?;
    let mask = physical_address_mask(physical_address_bits)?;
    if root.get() & PAGE_OFFSET_MASK != 0 || root.get() & !mask != 0 {
        return Err(X86PagingError::InvalidRootTable);
    }

    let pml4e = read_table_entry(memory, root, indices.pml4)?;
    if pml4e & PRESENT == 0 || pml4e & HUGE_PAGE != 0 {
        return Err(X86PagingError::NotMapped);
    }
    let pdpt = PhysAddr::new(pml4e & mask);
    let pdpte = read_table_entry(memory, pdpt, indices.pdpt)?;
    if pdpte & PRESENT == 0 {
        return Err(X86PagingError::NotMapped);
    }
    if pdpte & HUGE_PAGE != 0 {
        let size = PageSize::Size1GiB;
        let base = pdpte & mask & !(size.bytes().get() - 1);
        return Ok(Translation {
            physical_address: PhysAddr::new(
                base | (virtual_address.get() & (size.bytes().get() - 1)),
            ),
            page_size: size,
        });
    }

    let pd = PhysAddr::new(pdpte & mask);
    let pde = read_table_entry(memory, pd, indices.pd)?;
    if pde & PRESENT == 0 {
        return Err(X86PagingError::NotMapped);
    }
    if pde & HUGE_PAGE != 0 {
        let size = PageSize::Size2MiB;
        let base = pde & mask & !(size.bytes().get() - 1);
        return Ok(Translation {
            physical_address: PhysAddr::new(
                base | (virtual_address.get() & (size.bytes().get() - 1)),
            ),
            page_size: size,
        });
    }

    let pt = PhysAddr::new(pde & mask);
    let pte = read_table_entry(memory, pt, indices.pt)?;
    if pte & PRESENT == 0 {
        return Err(X86PagingError::NotMapped);
    }
    let base = pte & mask;
    Ok(Translation {
        physical_address: PhysAddr::new(base | (virtual_address.get() & 0xfff)),
        page_size: PageSize::Size4KiB,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const X86_64_4_LEVEL: AddressGeometry = match AddressGeometry::new(48) {
        Ok(value) => value,
        Err(_) => panic!("valid x86-64 geometry"),
    };

    #[derive(Default)]
    struct SyntheticTables(std::collections::BTreeMap<(u64, u16), u64>);

    impl PageTableMemory for SyntheticTables {
        fn read_entry(&self, table: PhysAddr, index: u16) -> Option<u64> {
            Some(self.0.get(&(table.get(), index)).copied().unwrap_or(0))
        }

        fn write_entry(&mut self, table: PhysAddr, index: u16, value: u64) -> bool {
            self.0.insert((table.get(), index), value);
            true
        }
    }

    fn synthetic_four_level_tables() -> SyntheticTables {
        let mut tables = SyntheticTables::default();
        // Root 0x1000 -> PDPT 0x2000 -> PD 0x3000 -> PT 0x4000.
        tables.write_entry(PhysAddr::new(0x1000), 256, 0x2000 | PRESENT | WRITABLE);
        tables.write_entry(PhysAddr::new(0x2000), 0, 0x3000 | PRESENT | WRITABLE);
        tables.write_entry(PhysAddr::new(0x3000), 0, 0x4000 | PRESENT | WRITABLE);
        tables
    }

    #[test]
    fn hhdm_page_table_accessor_rejects_invalid_table_addresses_without_dereferencing() {
        // SAFETY: This test only exercises rejected inputs; no table memory is accessed.
        let memory = unsafe { HhdmPageTableMemory::new(0xffff_8000_0000_0000, 32) }.unwrap();

        assert_eq!(memory.read_entry(PhysAddr::new(0x1000), 512), None);
        assert_eq!(memory.read_entry(PhysAddr::new(0x1001), 0), None);
        assert_eq!(memory.read_entry(PhysAddr::new(1u64 << 32), 0), None);

        // A valid physical table whose HHDM alias would overflow is also rejected.
        // SAFETY: The overflowing alias is rejected before any pointer is dereferenced.
        let overflowing = unsafe { HhdmPageTableMemory::new(u64::MAX - 0x7ff, 48) }.unwrap();
        assert_eq!(overflowing.read_entry(PhysAddr::new(0x1000), 0), None);
    }

    #[test]
    fn pmm_page_table_pool_zeroes_tracks_and_releases_frames() {
        use crate::boot::EarlyMemoryRange;
        use crate::pmm::FrameMetadata;
        use core::num::NonZeroU64;

        #[repr(align(4096))]
        struct Backing([u8; 8192]);

        let mut backing = Backing([0xa5; 8192]);
        let physical_base = 0x1000u64;
        let hhdm_offset = (backing.0.as_mut_ptr() as u64) - physical_base;
        let ranges = [EarlyMemoryRange {
            base: physical_base,
            length: 0x2000,
        }];
        let mut bitmap = [0u8; 1];
        let mut metadata = [FrameMetadata::default(); 2];
        let mut pmm = Pmm::new(
            &ranges,
            NonZeroU64::new(4096).unwrap(),
            &mut bitmap,
            &mut metadata,
        )
        .unwrap();

        // SAFETY: The aligned backing buffer is writable, exclusively borrowed,
        // and the synthetic HHDM maps both candidate frames into that buffer.
        let mut pool = unsafe { PageTableFramePool::<2>::new(&mut pmm, hhdm_offset, 48) }.unwrap();
        let frame = pool.allocate_zeroed_table().unwrap();
        assert_eq!(frame, PhysAddr::new(physical_base));
        assert_eq!(pool.owned_frame_count(), 1);
        assert!(backing.0[..4096].iter().all(|byte| *byte == 0));
        assert_eq!(pool.memory().read_entry(frame, 0), Some(0));

        // SAFETY: The test has not installed this frame in any page-table
        // hierarchy, so it is detached and has no possible TLB references.
        unsafe { pool.release_table_frame(frame) }.unwrap();
        assert_eq!(pool.owned_frame_count(), 0);
        assert_eq!(
            backing.0[..4096].iter().filter(|byte| **byte == 0).count(),
            4096
        );
    }

    #[test]
    fn pmm_page_table_pool_enforces_capacity_and_ownership() {
        use crate::boot::EarlyMemoryRange;
        use crate::pmm::FrameMetadata;
        use core::num::NonZeroU64;

        #[repr(align(4096))]
        struct Backing([u8; 8192]);

        let mut backing = Backing([0x5a; 8192]);
        let physical_base = 0x1000u64;
        let hhdm_offset = (backing.0.as_mut_ptr() as u64) - physical_base;
        let ranges = [EarlyMemoryRange {
            base: physical_base,
            length: 0x2000,
        }];
        let mut bitmap = [0u8; 1];
        let mut metadata = [FrameMetadata::default(); 2];
        let mut pmm = Pmm::new(
            &ranges,
            NonZeroU64::new(4096).unwrap(),
            &mut bitmap,
            &mut metadata,
        )
        .unwrap();

        // SAFETY: The synthetic HHDM maps the aligned backing buffer and no
        // frame is concurrently accessed.
        let mut pool = unsafe { PageTableFramePool::<1>::new(&mut pmm, hhdm_offset, 48) }.unwrap();
        let frame = pool.allocate_zeroed_table().unwrap();
        assert_eq!(
            pool.allocate_zeroed_table(),
            Err(PageTablePoolError::CapacityExhausted)
        );
        // SAFETY: The frame is not installed in any page-table tree.
        assert_eq!(
            unsafe { pool.release_table_frame(PhysAddr::new(0x9000)) },
            Err(PageTablePoolError::FrameNotOwned)
        );
        // SAFETY: The allocated frame remains detached in this test.
        unsafe { pool.release_table_frame(frame) }.unwrap();
    }

    #[test]
    fn hierarchy_builder_allocates_tables_and_maps_four_kib_leaf() {
        use crate::boot::EarlyMemoryRange;
        use crate::pmm::FrameMetadata;
        use core::num::NonZeroU64;

        #[repr(align(4096))]
        struct Backing([u8; 6 * 4096]);

        let mut backing = Backing([0xa5; 6 * 4096]);
        let physical_base = 0x1000u64;
        let hhdm_offset = (backing.0.as_mut_ptr() as u64) - physical_base;
        let ranges = [EarlyMemoryRange {
            base: physical_base,
            length: 6 * 4096,
        }];
        let mut bitmap = [0u8; 1];
        let mut metadata = [FrameMetadata::default(); 6];
        let mut pmm = Pmm::new(
            &ranges,
            NonZeroU64::new(4096).unwrap(),
            &mut bitmap,
            &mut metadata,
        )
        .unwrap();

        // SAFETY: The aligned backing buffer maps all candidate frames through
        // the synthetic HHDM and remains exclusively borrowed by this test.
        let mut pool = unsafe { PageTableFramePool::<4>::new(&mut pmm, hhdm_offset, 48) }.unwrap();
        let root = pool.allocate_zeroed_table().unwrap();
        let virtual_address = VirtAddr::new(0xffff_8000_1234_5000);
        // SAFETY: The root is private and has not been activated or published.
        unsafe {
            pool.map_leaf_with_tables(
                root,
                virtual_address,
                PhysAddr::new(0x9000),
                PageSize::Size4KiB,
                PageFlags::KERNEL_WRITABLE,
                48,
                true,
            )
        }
        .unwrap();

        let translated = translate(
            pool.memory(),
            root,
            VirtAddr::new(0xffff_8000_1234_5123),
            48,
        )
        .unwrap();
        assert_eq!(translated.physical_address, PhysAddr::new(0x9123));
        assert_eq!(translated.page_size, PageSize::Size4KiB);
        assert_eq!(pool.owned_frame_count(), 4);
    }

    #[test]
    fn hierarchy_builder_rolls_back_allocations_when_capacity_is_exhausted() {
        use crate::boot::EarlyMemoryRange;
        use crate::pmm::FrameMetadata;
        use core::num::NonZeroU64;

        #[repr(align(4096))]
        struct Backing([u8; 6 * 4096]);

        let mut backing = Backing([0; 6 * 4096]);
        let physical_base = 0x1000u64;
        let hhdm_offset = (backing.0.as_mut_ptr() as u64) - physical_base;
        let ranges = [EarlyMemoryRange {
            base: physical_base,
            length: 6 * 4096,
        }];
        let mut bitmap = [0u8; 1];
        let mut metadata = [FrameMetadata::default(); 6];
        let mut pmm = Pmm::new(
            &ranges,
            NonZeroU64::new(4096).unwrap(),
            &mut bitmap,
            &mut metadata,
        )
        .unwrap();

        // Three slots allow the root and only two of the three child tables.
        // SAFETY: The backing buffer is a valid private synthetic HHDM.
        let mut pool = unsafe { PageTableFramePool::<3>::new(&mut pmm, hhdm_offset, 48) }.unwrap();
        let root = pool.allocate_zeroed_table().unwrap();
        let result = unsafe {
            pool.map_leaf_with_tables(
                root,
                VirtAddr::new(0xffff_8000_1234_5000),
                PhysAddr::new(0x9000),
                PageSize::Size4KiB,
                PageFlags::KERNEL_WRITABLE,
                48,
                true,
            )
        };
        assert_eq!(
            result,
            Err(PageTableHierarchyError::Pool(
                PageTablePoolError::CapacityExhausted
            ))
        );
        assert_eq!(pool.owned_frame_count(), 1);
        assert_eq!(pool.memory().read_entry(root, 256), Some(0));
        assert_eq!(pool.pmm.free_frames(), 5);
    }

    #[test]
    fn hierarchy_unmap_reclaims_empty_owned_tables_bottom_up() {
        use crate::boot::EarlyMemoryRange;
        use crate::pmm::FrameMetadata;
        use core::num::NonZeroU64;

        #[repr(align(4096))]
        struct Backing([u8; 6 * 4096]);

        let mut backing = Backing([0; 6 * 4096]);
        let physical_base = 0x1000u64;
        let hhdm_offset = (backing.0.as_mut_ptr() as u64) - physical_base;
        let ranges = [EarlyMemoryRange {
            base: physical_base,
            length: 6 * 4096,
        }];
        let mut bitmap = [0u8; 1];
        let mut metadata = [FrameMetadata::default(); 6];
        let mut pmm = Pmm::new(
            &ranges,
            NonZeroU64::new(4096).unwrap(),
            &mut bitmap,
            &mut metadata,
        )
        .unwrap();

        // SAFETY: The backing buffer is a valid private synthetic HHDM.
        let mut pool = unsafe { PageTableFramePool::<4>::new(&mut pmm, hhdm_offset, 48) }.unwrap();
        let root = pool.allocate_zeroed_table().unwrap();
        let virtual_address = VirtAddr::new(0xffff_8000_1234_5000);
        // SAFETY: The root is private and remains inactive throughout the test.
        unsafe {
            pool.map_leaf_with_tables(
                root,
                virtual_address,
                PhysAddr::new(0x9000),
                PageSize::Size4KiB,
                PageFlags::KERNEL_WRITABLE,
                48,
                true,
            )
        }
        .unwrap();
        assert_eq!(pool.owned_frame_count(), 4);

        // SAFETY: No CPU can walk this private root and no TLB can reference it.
        unsafe {
            pool.unmap_leaf_with_reclaim(
                root,
                virtual_address,
                PageSize::Size4KiB,
                48,
            )
        }
        .unwrap();

        assert_eq!(pool.owned_frame_count(), 1);
        assert_eq!(pool.memory().read_entry(root, 256), Some(0));
        assert_eq!(pool.pmm.free_frames(), 5);
    }

    #[test]
    fn hhdm_page_table_accessor_rejects_invalid_physical_width() {
        // SAFETY: Constructor validation returns before the accessor can be used.
        assert!(matches!(
            unsafe { HhdmPageTableMemory::new(0xffff_8000_0000_0000, 53) },
            Err(X86PagingError::InvalidPhysicalAddressWidth)
        ));
    }

    #[test]
    fn user_leaf_requires_user_access_at_every_intermediate_level() {
        let mut tables = synthetic_four_level_tables();
        let root = PhysAddr::new(0x1000);
        let address = VirtAddr::new(0xffff_8000_0000_3000);

        assert_eq!(
            map_leaf(
                &mut tables,
                root,
                address,
                PhysAddr::new(0x9000),
                PageSize::Size4KiB,
                PageFlags::USER_READ_ONLY,
                48,
                true,
            ),
            Err(X86PagingError::InsufficientIntermediatePermissions)
        );

        for (table, index) in [(0x1000, 256), (0x2000, 0), (0x3000, 0)] {
            let entry = tables.read_entry(PhysAddr::new(table), index).unwrap();
            assert!(tables.write_entry(PhysAddr::new(table), index, entry | USER));
        }

        map_leaf(
            &mut tables,
            root,
            address,
            PhysAddr::new(0x9000),
            PageSize::Size4KiB,
            PageFlags::USER_READ_ONLY,
            48,
            true,
        )
        .unwrap();
        let leaf = tables.read_entry(PhysAddr::new(0x4000), 3).unwrap();
        assert_ne!(leaf & USER, 0);
    }

    #[test]
    fn two_mib_leaf_checks_both_parent_levels() {
        let mut tables = synthetic_four_level_tables();
        let root = PhysAddr::new(0x1000);
        let root_entry = tables.read_entry(root, 256).unwrap();
        assert!(tables.write_entry(root, 256, root_entry | USER));
        let pdpt_entry = tables.read_entry(PhysAddr::new(0x2000), 0).unwrap();

        assert_eq!(
            map_leaf(
                &mut tables,
                root,
                VirtAddr::new(0xffff_8000_0040_0000),
                PhysAddr::new(0x80_0000),
                PageSize::Size2MiB,
                PageFlags::USER_READ_ONLY,
                48,
                true,
            ),
            Err(X86PagingError::InsufficientIntermediatePermissions)
        );

        let pdpt = PhysAddr::new(0x2000);
        let updated_entry = pdpt_entry | USER;
        assert!(tables.write_entry(pdpt, 0, updated_entry));
        map_leaf(
            &mut tables,
            root,
            VirtAddr::new(0xffff_8000_0040_0000),
            PhysAddr::new(0x80_0000),
            PageSize::Size2MiB,
            PageFlags::USER_READ_ONLY,
            48,
            true,
        )
        .unwrap();
        let leaf = tables.read_entry(PhysAddr::new(0x3000), 2).unwrap();
        assert_ne!(leaf & USER, 0);
    }

    #[test]
    fn writable_leaf_requires_writable_intermediate_entries() {
        let mut tables = synthetic_four_level_tables();
        let root = PhysAddr::new(0x1000);
        let root_entry = tables.read_entry(root, 256).unwrap();
        assert!(tables.write_entry(root, 256, root_entry & !WRITABLE));

        assert_eq!(
            map_leaf(
                &mut tables,
                root,
                VirtAddr::new(0xffff_8000_0000_3000),
                PhysAddr::new(0x9000),
                PageSize::Size4KiB,
                PageFlags::KERNEL_WRITABLE,
                48,
                true,
            ),
            Err(X86PagingError::InsufficientIntermediatePermissions)
        );
    }

    #[test]
    fn executable_leaf_rejects_nx_at_every_intermediate_level() {
        let mut tables = synthetic_four_level_tables();
        let root = PhysAddr::new(0x1000);
        let address = VirtAddr::new(0xffff_8000_0000_3000);

        for (table, index) in [(0x1000, 256), (0x2000, 0), (0x3000, 0)] {
            let table_address = PhysAddr::new(table);
            let original = tables.read_entry(table_address, index).unwrap();
            assert!(tables.write_entry(table_address, index, original | NO_EXECUTE));

            assert_eq!(
                map_leaf(
                    &mut tables,
                    root,
                    address,
                    PhysAddr::new(0x9000),
                    PageSize::Size4KiB,
                    PageFlags::KERNEL_EXECUTABLE,
                    48,
                    true,
                ),
                Err(X86PagingError::InsufficientIntermediateExecutePermission)
            );

            assert!(tables.write_entry(table_address, index, original));
        }
    }

    #[test]
    fn maps_translates_and_unmaps_4k_leaf_in_synthetic_tables() {
        let mut tables = synthetic_four_level_tables();
        let root = PhysAddr::new(0x1000);
        let virtual_address = VirtAddr::new(0xffff_8000_0000_3000);
        map_leaf(
            &mut tables,
            root,
            virtual_address,
            PhysAddr::new(0x9000),
            PageSize::Size4KiB,
            PageFlags::KERNEL_WRITABLE,
            48,
            true,
        )
        .unwrap();
        let translated =
            translate(&tables, root, VirtAddr::new(0xffff_8000_0000_3123), 48).unwrap();
        assert_eq!(translated.physical_address, PhysAddr::new(0x9123));
        assert_eq!(translated.page_size, PageSize::Size4KiB);
        assert_eq!(
            map_leaf(
                &mut tables,
                root,
                virtual_address,
                PhysAddr::new(0xa000),
                PageSize::Size4KiB,
                PageFlags::KERNEL_READ_ONLY,
                48,
                true,
            ),
            Err(X86PagingError::AlreadyMapped)
        );
        unmap_leaf(&mut tables, root, virtual_address, PageSize::Size4KiB, 48).unwrap();
        assert_eq!(
            translate(&tables, root, virtual_address, 48),
            Err(X86PagingError::NotMapped)
        );
    }

    #[test]
    fn translates_existing_2m_and_1g_huge_pages() {
        let mut tables = synthetic_four_level_tables();
        // Replace PDPT entry with a 1 GiB leaf.
        tables.write_entry(PhysAddr::new(0x2000), 1, 0x4000_0000 | PRESENT | HUGE_PAGE);
        let one_gib = translate(
            &tables,
            PhysAddr::new(0x1000),
            VirtAddr::new(0xffff_8000_5234_5678),
            48,
        )
        .unwrap();
        assert_eq!(one_gib.page_size, PageSize::Size1GiB);
        assert_eq!(one_gib.physical_address, PhysAddr::new(0x5234_5678));

        // Restore a table at PDPT[0], then install a 2 MiB leaf in PD[2].
        tables.write_entry(PhysAddr::new(0x2000), 1, 0);
        tables.write_entry(PhysAddr::new(0x3000), 2, 0x80_0000 | PRESENT | HUGE_PAGE);
        let two_mib = translate(
            &tables,
            PhysAddr::new(0x1000),
            VirtAddr::new(0xffff_8000_0040_1234),
            48,
        )
        .unwrap();
        assert_eq!(two_mib.page_size, PageSize::Size2MiB);
        assert_eq!(two_mib.physical_address, PhysAddr::new(0x80_1234));
    }

    #[test]
    fn rejects_missing_intermediate_tables_and_invalid_root() {
        let mut tables = SyntheticTables::default();
        assert_eq!(
            map_leaf(
                &mut tables,
                PhysAddr::new(0x1000),
                VirtAddr::new(0xffff_8000_0000_0000),
                PhysAddr::new(0x8000),
                PageSize::Size4KiB,
                PageFlags::KERNEL_READ_ONLY,
                48,
                true,
            ),
            Err(X86PagingError::MissingIntermediateTable)
        );
        assert_eq!(
            translate(
                &tables,
                PhysAddr::new(0x1001),
                VirtAddr::new(0xffff_8000_0000_0000),
                48
            ),
            Err(X86PagingError::InvalidRootTable)
        );
    }

    #[test]
    fn encodes_read_only_non_executable_4k_leaf() {
        let entry = PageTableEntry::encode_leaf(
            PhysAddr::new(0x1234_5000),
            PageSize::Size4KiB,
            PageFlags::KERNEL_READ_ONLY,
            48,
            true,
        )
        .unwrap();
        assert!(entry.is_present());
        assert!(!entry.is_writable());
        assert!(entry.is_no_execute());
        assert!(!entry.is_huge_page());
        assert_eq!(entry.physical_base(), PhysAddr::new(0x1234_5000));
    }

    #[test]
    fn encodes_executable_and_writable_leaf_permissions() {
        let executable = PageTableEntry::encode_leaf(
            PhysAddr::new(0x4000),
            PageSize::Size4KiB,
            PageFlags::KERNEL_EXECUTABLE,
            48,
            true,
        )
        .unwrap();
        assert!(!executable.is_no_execute());

        let writable = PageTableEntry::encode_leaf(
            PhysAddr::new(0x8000),
            PageSize::Size4KiB,
            PageFlags::KERNEL_WRITABLE,
            48,
            true,
        )
        .unwrap();
        assert!(writable.is_writable());
        assert!(writable.is_no_execute());
    }

    #[test]
    fn sets_huge_page_bit_for_2m_and_1g_leaves() {
        for (base, size) in [
            (0x20_0000, PageSize::Size2MiB),
            (0x4000_0000, PageSize::Size1GiB),
        ] {
            let entry = PageTableEntry::encode_leaf(
                PhysAddr::new(base),
                size,
                PageFlags::KERNEL_READ_ONLY,
                48,
                true,
            )
            .unwrap();
            assert!(entry.is_huge_page());
            assert_eq!(entry.physical_base(), PhysAddr::new(base));
        }
    }

    #[test]
    fn rejects_misaligned_and_out_of_range_physical_addresses() {
        assert_eq!(
            PageTableEntry::encode_leaf(
                PhysAddr::new(0x20_1000),
                PageSize::Size2MiB,
                PageFlags::KERNEL_READ_ONLY,
                48,
                true,
            ),
            Err(X86PagingError::MisalignedPhysicalAddress)
        );
        assert_eq!(
            PageTableEntry::encode_leaf(
                PhysAddr::new(1u64 << 48),
                PageSize::Size4KiB,
                PageFlags::KERNEL_READ_ONLY,
                48,
                true,
            ),
            Err(X86PagingError::PhysicalAddressOutOfRange)
        );
        assert_eq!(
            PageTableEntry::encode_leaf(
                PhysAddr::new(0x1000),
                PageSize::Size4KiB,
                PageFlags::KERNEL_READ_ONLY,
                53,
                true,
            ),
            Err(X86PagingError::InvalidPhysicalAddressWidth)
        );
    }

    #[test]
    fn requires_nx_support_for_non_executable_mappings() {
        assert_eq!(
            PageTableEntry::encode_leaf(
                PhysAddr::new(0x1000),
                PageSize::Size4KiB,
                PageFlags::KERNEL_READ_ONLY,
                48,
                false,
            ),
            Err(X86PagingError::NoExecuteUnavailable)
        );
    }

    #[test]
    fn extracts_four_level_indices_from_canonical_addresses() {
        let high_half =
            PageTableIndices::from_address(VirtAddr::new(0xffff_8000_0000_0000), X86_64_4_LEVEL)
                .unwrap();
        assert_eq!(
            high_half,
            PageTableIndices {
                pml4: 256,
                pdpt: 0,
                pd: 0,
                pt: 0,
            }
        );

        let final_page =
            PageTableIndices::from_address(VirtAddr::new(0xffff_ffff_ffff_f000), X86_64_4_LEVEL)
                .unwrap();
        assert_eq!(
            final_page,
            PageTableIndices {
                pml4: 511,
                pdpt: 511,
                pd: 511,
                pt: 511,
            }
        );
    }

    #[test]
    fn rejects_noncanonical_addresses_and_wrong_geometry() {
        assert_eq!(
            PageTableIndices::from_address(VirtAddr::new(0x0000_8000_0000_0000), X86_64_4_LEVEL),
            Err(X86PagingError::NonCanonicalAddress)
        );
        let geometry = AddressGeometry::new(57).unwrap();
        assert_eq!(
            PageTableIndices::from_address(VirtAddr::new(0xffff_8000_0000_0000), geometry),
            Err(X86PagingError::InvalidAddressGeometry)
        );
    }
}
