//! Architecture-neutral virtual-memory request validation.
//!
//! This module describes page mappings without encoding a hardware page-table
//! format. Architecture backends must still implement table allocation,
//! descriptor encoding, TLB maintenance, and synchronization.

use core::num::NonZeroU64;

use crate::memory::{PhysAddr, PhysRange, VirtAddr};

/// Supported base and huge-page sizes in the initial VMM contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageSize {
    Size4KiB,
    Size2MiB,
    Size1GiB,
}

impl PageSize {
    pub const fn bytes(self) -> NonZeroU64 {
        let value = match self {
            Self::Size4KiB => 4 * 1024,
            Self::Size2MiB => 2 * 1024 * 1024,
            Self::Size1GiB => 1024 * 1024 * 1024,
        };
        // SAFETY: Every enum variant contains a non-zero page size.
        unsafe { NonZeroU64::new_unchecked(value) }
    }
}

/// Errors from virtual-memory contract validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmmError {
    InvalidVirtualAddressWidth,
    NonCanonicalAddress,
    NonCanonicalRange,
    EmptyRange,
    AddressOverflow,
    ReversedRange,
    LengthMismatch,
    MisalignedVirtualAddress,
    MisalignedPhysicalAddress,
    MisalignedLength,
    UnknownPageFlags,
    WritableExecutableMapping,
    UserGlobalMapping,
}

/// Canonical-address rules for a configured virtual-address width.
///
/// For x86-64 four-level paging, use 48 bits. The width is configurable so
/// later architecture backends can select the active translation regime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressGeometry {
    virtual_address_bits: u8,
}

impl AddressGeometry {
    pub const fn new(virtual_address_bits: u8) -> Result<Self, VmmError> {
        if virtual_address_bits < 2 || virtual_address_bits > 64 {
            return Err(VmmError::InvalidVirtualAddressWidth);
        }
        Ok(Self {
            virtual_address_bits,
        })
    }

    pub const fn virtual_address_bits(self) -> u8 {
        self.virtual_address_bits
    }

    pub const fn is_canonical(self, address: VirtAddr) -> bool {
        if self.virtual_address_bits == 64 {
            return true;
        }
        let bits = self.virtual_address_bits as u32;
        let sign = (address.get() >> (bits - 1)) & 1;
        let upper = address.get() >> bits;
        if sign == 0 {
            upper == 0
        } else {
            upper == (u64::MAX >> bits)
        }
    }

    const fn is_upper_canonical_half(self, address: VirtAddr) -> bool {
        if self.virtual_address_bits == 64 {
            false
        } else {
            ((address.get() >> (self.virtual_address_bits as u32 - 1)) & 1) != 0
        }
    }

    pub const fn validate_range(self, range: VirtRange) -> Result<(), VmmError> {
        let last = VirtAddr::new(range.end().get() - 1);
        if !self.is_canonical(range.start()) || !self.is_canonical(last) {
            return Err(VmmError::NonCanonicalAddress);
        }
        if self.is_upper_canonical_half(range.start()) != self.is_upper_canonical_half(last) {
            return Err(VmmError::NonCanonicalRange);
        }
        Ok(())
    }
}

/// A non-empty, half-open virtual-address range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtRange {
    start: VirtAddr,
    end: VirtAddr,
}

impl VirtRange {
    pub const fn from_start_len(start: VirtAddr, length: NonZeroU64) -> Result<Self, VmmError> {
        match start.checked_add(length.get()) {
            Some(end) => Ok(Self { start, end }),
            None => Err(VmmError::AddressOverflow),
        }
    }

    pub const fn from_bounds(start: VirtAddr, end: VirtAddr) -> Result<Self, VmmError> {
        if end.get() < start.get() {
            return Err(VmmError::ReversedRange);
        }
        if end.get() == start.get() {
            return Err(VmmError::EmptyRange);
        }
        Ok(Self { start, end })
    }

    pub const fn start(self) -> VirtAddr {
        self.start
    }

    /// Exclusive end address.
    pub const fn end(self) -> VirtAddr {
        self.end
    }

    pub const fn len(self) -> u64 {
        self.end.get() - self.start.get()
    }
}

/// Hardware-independent mapping permissions and cache attributes.
///
/// Read access is implicit in the initial x86-64 and AArch64 backends. A
/// writable + executable mapping is rejected to enforce the default W^X rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct PageFlags(u16);

impl PageFlags {
    pub const WRITABLE: u16 = 1 << 0;
    pub const EXECUTABLE: u16 = 1 << 1;
    pub const USER: u16 = 1 << 2;
    pub const GLOBAL: u16 = 1 << 3;
    pub const WRITE_THROUGH: u16 = 1 << 4;
    pub const CACHE_DISABLE: u16 = 1 << 5;

    const KNOWN_BITS: u16 = Self::WRITABLE
        | Self::EXECUTABLE
        | Self::USER
        | Self::GLOBAL
        | Self::WRITE_THROUGH
        | Self::CACHE_DISABLE;

    pub const KERNEL_READ_ONLY: Self = Self(0);
    pub const KERNEL_WRITABLE: Self = Self(Self::WRITABLE);
    pub const KERNEL_EXECUTABLE: Self = Self(Self::EXECUTABLE);
    pub const USER_READ_ONLY: Self = Self(Self::USER);

    pub const fn from_bits(bits: u16) -> Result<Self, VmmError> {
        if bits & !Self::KNOWN_BITS != 0 {
            return Err(VmmError::UnknownPageFlags);
        }
        if bits & Self::WRITABLE != 0 && bits & Self::EXECUTABLE != 0 {
            return Err(VmmError::WritableExecutableMapping);
        }
        if bits & Self::USER != 0 && bits & Self::GLOBAL != 0 {
            return Err(VmmError::UserGlobalMapping);
        }
        Ok(Self(bits))
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn is_writable(self) -> bool {
        self.0 & Self::WRITABLE != 0
    }

    pub const fn is_executable(self) -> bool {
        self.0 & Self::EXECUTABLE != 0
    }

    pub const fn is_user(self) -> bool {
        self.0 & Self::USER != 0
    }
}

/// A validated request to map equal-length virtual and physical intervals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MappingRequest {
    virtual_range: VirtRange,
    physical_range: PhysRange,
    page_size: PageSize,
    flags: PageFlags,
}

impl MappingRequest {
    pub fn new(
        virtual_range: VirtRange,
        physical_range: PhysRange,
        page_size: PageSize,
        flags: PageFlags,
        geometry: AddressGeometry,
    ) -> Result<Self, VmmError> {
        if virtual_range.len() != physical_range.len() {
            return Err(VmmError::LengthMismatch);
        }
        let page = page_size.bytes().get();
        if virtual_range.start().get() % page != 0 {
            return Err(VmmError::MisalignedVirtualAddress);
        }
        if !physical_range.start().is_aligned(page_size.bytes())
            || !physical_range.end().is_aligned(page_size.bytes())
        {
            return Err(VmmError::MisalignedPhysicalAddress);
        }
        if virtual_range.len() % page != 0 {
            return Err(VmmError::MisalignedLength);
        }
        geometry.validate_range(virtual_range)?;
        Ok(Self {
            virtual_range,
            physical_range,
            page_size,
            flags,
        })
    }

    pub const fn virtual_range(self) -> VirtRange {
        self.virtual_range
    }

    pub const fn physical_range(self) -> PhysRange {
        self.physical_range
    }

    pub const fn page_size(self) -> PageSize {
        self.page_size
    }

    pub const fn flags(self) -> PageFlags {
        self.flags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: NonZeroU64 = match NonZeroU64::new(4096) {
        Some(value) => value,
        None => panic!("page size must be non-zero"),
    };
    const X86_64_4_LEVEL: AddressGeometry = match AddressGeometry::new(48) {
        Ok(value) => value,
        Err(_) => panic!("valid address geometry"),
    };

    fn virtual_range(base: u64, length: u64) -> VirtRange {
        VirtRange::from_start_len(
            VirtAddr::new(base),
            NonZeroU64::new(length).expect("non-empty test range"),
        )
        .unwrap()
    }

    fn physical_range(base: u64, length: u64) -> PhysRange {
        PhysRange::from_start_len(
            PhysAddr::new(base),
            NonZeroU64::new(length).expect("non-empty test range"),
        )
        .unwrap()
    }

    #[test]
    fn validates_x86_64_canonical_addresses() {
        assert!(X86_64_4_LEVEL.is_canonical(VirtAddr::new(0x0000_7fff_ffff_ffff)));
        assert!(X86_64_4_LEVEL.is_canonical(VirtAddr::new(0xffff_8000_0000_0000)));
        assert!(!X86_64_4_LEVEL.is_canonical(VirtAddr::new(0x0000_8000_0000_0000)));
        assert!(!X86_64_4_LEVEL.is_canonical(VirtAddr::new(0xffff_7fff_ffff_ffff)));
    }

    #[test]
    fn rejects_ranges_crossing_the_canonical_hole() {
        let range = virtual_range(0x0000_7fff_ffff_f000, 0x2000);
        assert_eq!(
            X86_64_4_LEVEL.validate_range(range),
            Err(VmmError::NonCanonicalAddress)
        );
    }

    #[test]
    fn validates_page_mapping_alignment_and_lengths() {
        let request = MappingRequest::new(
            virtual_range(0xffff_8000_0000_0000, PAGE.get()),
            physical_range(0x20_0000, PAGE.get()),
            PageSize::Size4KiB,
            PageFlags::KERNEL_WRITABLE,
            X86_64_4_LEVEL,
        )
        .unwrap();
        assert_eq!(request.page_size(), PageSize::Size4KiB);
        assert_eq!(request.flags(), PageFlags::KERNEL_WRITABLE);
    }

    #[test]
    fn rejects_misaligned_virtual_and_physical_ranges() {
        assert_eq!(
            MappingRequest::new(
                virtual_range(0xffff_8000_0000_0001, PAGE.get()),
                physical_range(0x20_0000, PAGE.get()),
                PageSize::Size4KiB,
                PageFlags::KERNEL_READ_ONLY,
                X86_64_4_LEVEL,
            ),
            Err(VmmError::MisalignedVirtualAddress)
        );
        assert_eq!(
            MappingRequest::new(
                virtual_range(0xffff_8000_0000_0000, PAGE.get()),
                physical_range(0x20_0001, PAGE.get()),
                PageSize::Size4KiB,
                PageFlags::KERNEL_READ_ONLY,
                X86_64_4_LEVEL,
            ),
            Err(VmmError::MisalignedPhysicalAddress)
        );
    }

    #[test]
    fn rejects_mapping_length_mismatch_and_invalid_geometry() {
        assert_eq!(
            MappingRequest::new(
                virtual_range(0xffff_8000_0000_0000, PAGE.get()),
                physical_range(0x20_0000, PAGE.get() * 2),
                PageSize::Size4KiB,
                PageFlags::KERNEL_READ_ONLY,
                X86_64_4_LEVEL,
            ),
            Err(VmmError::LengthMismatch)
        );
        assert_eq!(
            AddressGeometry::new(1),
            Err(VmmError::InvalidVirtualAddressWidth)
        );
    }

    #[test]
    fn enforces_wx_and_rejects_unknown_permission_bits() {
        assert_eq!(
            PageFlags::from_bits(PageFlags::WRITABLE | PageFlags::EXECUTABLE),
            Err(VmmError::WritableExecutableMapping)
        );
        assert_eq!(
            PageFlags::from_bits(1 << 15),
            Err(VmmError::UnknownPageFlags)
        );
        assert_eq!(
            PageFlags::from_bits(PageFlags::USER | PageFlags::GLOBAL),
            Err(VmmError::UserGlobalMapping)
        );
    }

    #[test]
    fn supports_large_page_alignment_contracts() {
        let large_page = PageSize::Size2MiB.bytes().get();
        let request = MappingRequest::new(
            virtual_range(0xffff_8000_0000_0000, large_page),
            physical_range(large_page * 2, large_page),
            PageSize::Size2MiB,
            PageFlags::KERNEL_READ_ONLY,
            X86_64_4_LEVEL,
        )
        .unwrap();
        assert_eq!(request.virtual_range().len(), large_page);
    }
}
