//! Strongly typed address and physical-range primitives.
//!
//! These types prevent accidental interchange of physical and virtual
//! addresses in common kernel code. They do not validate that an address is
//! mapped, canonical, or backed by RAM; those checks belong to the HAL/VMM/PMM.

use core::num::NonZeroU64;

/// A physical address. This is an integer address, not a dereferenceable pointer.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
#[repr(transparent)]
pub struct PhysAddr(u64);

impl PhysAddr {
    pub const fn new(value: u64) -> Self { Self(value) }
    pub const fn get(self) -> u64 { self.0 }
    pub const fn checked_add(self, offset: u64) -> Option<Self> {
        match self.0.checked_add(offset) { Some(value) => Some(Self(value)), None => None }
    }
    pub const fn is_aligned(self, alignment: NonZeroU64) -> bool {
        self.0 % alignment.get() == 0
    }
    pub const fn align_down(self, alignment: NonZeroU64) -> Self {
        Self(self.0 - (self.0 % alignment.get()))
    }
    pub const fn align_up(self, alignment: NonZeroU64) -> Option<Self> {
        let rem = self.0 % alignment.get();
        if rem == 0 { Some(self) } else { self.checked_add(alignment.get() - rem) }
    }
}

/// A virtual address. Mapping and architecture validity are checked elsewhere.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
#[repr(transparent)]
pub struct VirtAddr(u64);

impl VirtAddr {
    pub const fn new(value: u64) -> Self { Self(value) }
    pub const fn get(self) -> u64 { self.0 }
    pub const fn checked_add(self, offset: u64) -> Option<Self> {
        match self.0.checked_add(offset) { Some(value) => Some(Self(value)), None => None }
    }
}

/// A non-empty, half-open physical address range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysRange { start: PhysAddr, end: PhysAddr }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysRangeError { Empty, Overflow, Reversed }

impl PhysRange {
    pub const fn from_start_len(start: PhysAddr, length: NonZeroU64) -> Result<Self, PhysRangeError> {
        match start.checked_add(length.get()) {
            Some(end) => Ok(Self { start, end }),
            None => Err(PhysRangeError::Overflow),
        }
    }
    pub const fn from_bounds(start: PhysAddr, end: PhysAddr) -> Result<Self, PhysRangeError> {
        if end.get() < start.get() { return Err(PhysRangeError::Reversed); }
        if end.get() == start.get() { return Err(PhysRangeError::Empty); }
        Ok(Self { start, end })
    }
    pub const fn start(self) -> PhysAddr { self.start }
    pub const fn end(self) -> PhysAddr { self.end }
    pub const fn len(self) -> u64 { self.end.get() - self.start.get() }
    pub const fn contains(self, address: PhysAddr) -> bool {
        address.get() >= self.start.get() && address.get() < self.end.get()
    }
    pub const fn overlaps(self, other: Self) -> bool {
        self.start.get() < other.end.get() && other.start.get() < self.end.get()
    }
    pub const fn is_page_aligned(self, page_size: NonZeroU64) -> bool {
        self.start.is_aligned(page_size) && self.end.is_aligned(page_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::num::NonZeroU64;
    fn nz(value: u64) -> NonZeroU64 { NonZeroU64::new(value).expect("non-zero test value") }

    #[test]
    fn address_types_keep_physical_and_virtual_domains_distinct() {
        assert_eq!(PhysAddr::new(0x1000).get(), 0x1000);
        assert_eq!(VirtAddr::new(0xffff_8000_0000_1000).get(), 0xffff_8000_0000_1000);
    }
    #[test]
    fn checked_alignment_handles_boundaries_and_overflow() {
        let address = PhysAddr::new(0x1001);
        assert_eq!(address.align_down(nz(0x1000)).get(), 0x1000);
        assert_eq!(address.align_up(nz(0x1000)).unwrap().get(), 0x2000);
        assert!(PhysAddr::new(u64::MAX - 1).align_up(nz(4096)).is_none());
    }
    #[test]
    fn ranges_use_half_open_semantics() {
        let range = PhysRange::from_start_len(PhysAddr::new(0x1000), nz(0x2000)).unwrap();
        assert!(range.contains(PhysAddr::new(0x1000)));
        assert!(range.contains(PhysAddr::new(0x2fff)));
        assert!(!range.contains(PhysAddr::new(0x3000)));
        assert_eq!(range.len(), 0x2000);
    }
    #[test]
    fn ranges_detect_overlap_but_not_adjacency() {
        let first = PhysRange::from_start_len(PhysAddr::new(0x1000), nz(0x1000)).unwrap();
        let adjacent = PhysRange::from_start_len(PhysAddr::new(0x2000), nz(0x1000)).unwrap();
        let overlap = PhysRange::from_start_len(PhysAddr::new(0x1800), nz(0x1000)).unwrap();
        assert!(!first.overlaps(adjacent));
        assert!(first.overlaps(overlap));
    }
    #[test]
    fn rejects_empty_reversed_and_overflowing_ranges() {
        assert_eq!(PhysRange::from_bounds(PhysAddr::new(4), PhysAddr::new(4)), Err(PhysRangeError::Empty));
        assert_eq!(PhysRange::from_bounds(PhysAddr::new(8), PhysAddr::new(4)), Err(PhysRangeError::Reversed));
        assert_eq!(PhysRange::from_start_len(PhysAddr::new(u64::MAX - 3), nz(8)), Err(PhysRangeError::Overflow));
    }
}
