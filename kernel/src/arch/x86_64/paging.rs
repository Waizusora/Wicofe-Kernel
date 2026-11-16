//! x86-64 four-level paging descriptor primitives.
//!
//! This module encodes leaf entries and extracts page-table indices. It does
//! not allocate page tables, write CR3, modify active mappings, or invalidate
//! TLB entries. Callers must preserve the bootloader-provided mappings until
//! a complete replacement address space has been constructed and validated.

use crate::memory::{PhysAddr, VirtAddr};
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

#[cfg(test)]
mod tests {
    use super::*;

    const X86_64_4_LEVEL: AddressGeometry = match AddressGeometry::new(48) {
        Ok(value) => value,
        Err(_) => panic!("valid x86-64 geometry"),
    };

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
        let high_half = PageTableIndices::from_address(
            VirtAddr::new(0xffff_8000_0000_0000),
            X86_64_4_LEVEL,
        )
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

        let final_page = PageTableIndices::from_address(
            VirtAddr::new(0xffff_ffff_ffff_f000),
            X86_64_4_LEVEL,
        )
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
            PageTableIndices::from_address(
                VirtAddr::new(0x0000_8000_0000_0000),
                X86_64_4_LEVEL
            ),
            Err(X86PagingError::NonCanonicalAddress)
        );
        let geometry = AddressGeometry::new(57).unwrap();
        assert_eq!(
            PageTableIndices::from_address(VirtAddr::new(0xffff_8000_0000_0000), geometry),
            Err(X86PagingError::InvalidAddressGeometry)
        );
    }
}
