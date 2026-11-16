//! Initial single-frame physical memory manager.
//!
//! The bitmap is the authoritative allocation state. Per-frame metadata is
//! reserved for ownership diagnostics and future allocator extensions.
//! This implementation is single-core only: callers must serialize access
//! before enabling interrupts or starting additional CPUs.
//!
//! Only pass ranges produced by the validated early-memory pipeline after
//! kernel, module, framebuffer, and PMM metadata reservations have been
//! removed. This module does not inspect the boot memory map itself.

use core::num::NonZeroU64;

use crate::boot::EarlyMemoryRange;
use crate::memory::PhysAddr;

/// Per-frame diagnostic metadata. The bitmap remains the source of truth
/// for whether a frame is allocated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct FrameMetadata {
    /// Caller-defined owner or subsystem tag; zero means no tag.
    pub owner_tag: u64,
    /// Caller-defined diagnostic flags.
    pub flags: u32,
    /// Caller-defined allocation identifier.
    pub allocation_id: u32,
}

/// Errors returned by the initial physical frame allocator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmmError {
    InvalidPageSize,
    InvalidRanges,
    ArithmeticOverflow,
    InsufficientBitmap { required: usize, available: usize },
    InsufficientFrameMetadata { required: usize, available: usize },
    OutOfMemory,
    UnalignedAddress,
    UntrackedFrame,
    DoubleFree,
}

/// A bounded bitmap allocator over discontiguous physical-memory ranges.
///
/// The caller supplies storage for both the bitmap and per-frame metadata.
/// Initialization touches only the portions required by the tracked frame
/// count. The ranges must already exclude all reservations and the metadata
/// backing allocation itself.
pub struct Pmm<'a> {
    ranges: &'a [EarlyMemoryRange],
    bitmap: &'a mut [u8],
    metadata: &'a mut [FrameMetadata],
    page_size: u64,
    frame_count: usize,
    next_hint: usize,
    free_frames: usize,
}

impl<'a> Pmm<'a> {
    /// Initialize a single-core PMM over validated, sorted, page-aligned
    /// candidate ranges.
    pub fn new(
        ranges: &'a [EarlyMemoryRange],
        page_size: NonZeroU64,
        bitmap: &'a mut [u8],
        metadata: &'a mut [FrameMetadata],
    ) -> Result<Self, PmmError> {
        let page = page_size.get();
        if !page.is_power_of_two() {
            return Err(PmmError::InvalidPageSize);
        }

        let mut total_frames = 0usize;
        let mut previous_end = None;
        for range in ranges {
            if range.length == 0 || range.base % page != 0 || range.length % page != 0 {
                return Err(PmmError::InvalidRanges);
            }
            let end = range
                .base
                .checked_add(range.length)
                .ok_or(PmmError::ArithmeticOverflow)?;
            if previous_end.is_some_and(|previous| range.base < previous) {
                return Err(PmmError::InvalidRanges);
            }
            previous_end = Some(end);
            let frames = usize::try_from(range.length / page)
                .map_err(|_| PmmError::ArithmeticOverflow)?;
            total_frames = total_frames
                .checked_add(frames)
                .ok_or(PmmError::ArithmeticOverflow)?;
        }
        if total_frames == 0 {
            return Err(PmmError::InvalidRanges);
        }

        let bitmap_bytes = total_frames
            .checked_add(7)
            .ok_or(PmmError::ArithmeticOverflow)?
            / 8;
        if bitmap.len() < bitmap_bytes {
            return Err(PmmError::InsufficientBitmap {
                required: bitmap_bytes,
                available: bitmap.len(),
            });
        }
        if metadata.len() < total_frames {
            return Err(PmmError::InsufficientFrameMetadata {
                required: total_frames,
                available: metadata.len(),
            });
        }

        bitmap[..bitmap_bytes].fill(0);
        metadata[..total_frames].fill(FrameMetadata::default());

        let remainder = total_frames % 8;
        if remainder != 0 {
            bitmap[bitmap_bytes - 1] = !((1u8 << remainder) - 1);
        }

        Ok(Self {
            ranges,
            bitmap,
            metadata,
            page_size: page,
            frame_count: total_frames,
            next_hint: 0,
            free_frames: total_frames,
        })
    }

    /// Allocate one physical frame and attach a caller-defined owner tag.
    pub fn allocate_frame(&mut self, owner_tag: u64) -> Result<PhysAddr, PmmError> {
        if self.free_frames == 0 {
            return Err(PmmError::OutOfMemory);
        }

        for offset in 0..self.frame_count {
            let index = self.next_hint.wrapping_add(offset) % self.frame_count;
            if !self.is_allocated(index) {
                self.set_allocated(index, true);
                self.metadata[index] = FrameMetadata {
                    owner_tag,
                    flags: 0,
                    allocation_id: 0,
                };
                self.free_frames -= 1;
                self.next_hint = (index + 1) % self.frame_count;
                return self.physical_address(index).ok_or(PmmError::ArithmeticOverflow);
            }
        }
        Err(PmmError::OutOfMemory)
    }

    /// Release one previously allocated frame.
    pub fn free_frame(&mut self, address: PhysAddr) -> Result<(), PmmError> {
        let raw = address.get();
        if raw % self.page_size != 0 {
            return Err(PmmError::UnalignedAddress);
        }
        let index = self.frame_index(raw).ok_or(PmmError::UntrackedFrame)?;
        if !self.is_allocated(index) {
            return Err(PmmError::DoubleFree);
        }

        self.set_allocated(index, false);
        self.metadata[index] = FrameMetadata::default();
        self.free_frames += 1;
        if index < self.next_hint {
            self.next_hint = index;
        }
        Ok(())
    }

    /// Total number of frames represented by this allocator.
    pub const fn total_frames(&self) -> usize {
        self.frame_count
    }

    /// Number of currently free frames.
    pub const fn free_frames(&self) -> usize {
        self.free_frames
    }

    /// Number of currently allocated frames.
    pub const fn allocated_frames(&self) -> usize {
        self.frame_count - self.free_frames
    }

    /// Return diagnostic metadata for a tracked frame address.
    pub fn frame_metadata(&self, address: PhysAddr) -> Result<FrameMetadata, PmmError> {
        let raw = address.get();
        if raw % self.page_size != 0 {
            return Err(PmmError::UnalignedAddress);
        }
        let index = self.frame_index(raw).ok_or(PmmError::UntrackedFrame)?;
        Ok(self.metadata[index])
    }

    fn is_allocated(&self, index: usize) -> bool {
        self.bitmap[index / 8] & (1 << (index % 8)) != 0
    }

    fn set_allocated(&mut self, index: usize, allocated: bool) {
        let mask = 1 << (index % 8);
        if allocated {
            self.bitmap[index / 8] |= mask;
        } else {
            self.bitmap[index / 8] &= !mask;
        }
    }

    fn physical_address(&self, index: usize) -> Option<PhysAddr> {
        let mut remaining = index;
        for range in self.ranges {
            let count = usize::try_from(range.length / self.page_size).ok()?;
            if remaining < count {
                let offset = u64::try_from(remaining).ok()?.checked_mul(self.page_size)?;
                return range.base.checked_add(offset).map(PhysAddr::new);
            }
            remaining -= count;
        }
        None
    }

    fn frame_index(&self, address: u64) -> Option<usize> {
        let mut index_base = 0usize;
        for range in self.ranges {
            let end = range.base.checked_add(range.length)?;
            if address >= range.base && address < end {
                let frame_offset = usize::try_from((address - range.base) / self.page_size).ok()?;
                return index_base.checked_add(frame_offset);
            }
            let frames = usize::try_from(range.length / self.page_size).ok()?;
            index_base = index_base.checked_add(frames)?;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: NonZeroU64 = match NonZeroU64::new(0x1000) {
        Some(value) => value,
        None => panic!("page size must be non-zero"),
    };

    const fn range(base: u64, length: u64) -> EarlyMemoryRange {
        EarlyMemoryRange { base, length }
    }

    #[test]
    fn allocates_across_discontiguous_ranges_and_tracks_metadata() {
        let ranges = [range(0x1000, 0x2000), range(0x8000, 0x2000)];
        let mut bitmap = [0u8; 1];
        let mut metadata = [FrameMetadata::default(); 4];
        let mut pmm = Pmm::new(&ranges, PAGE, &mut bitmap, &mut metadata).unwrap();

        assert_eq!(pmm.total_frames(), 4);
        assert_eq!(pmm.free_frames(), 4);
        let first = pmm.allocate_frame(0x5749434f).unwrap();
        let second = pmm.allocate_frame(7).unwrap();
        let third = pmm.allocate_frame(8).unwrap();
        assert_eq!(first, PhysAddr::new(0x1000));
        assert_eq!(second, PhysAddr::new(0x2000));
        assert_eq!(third, PhysAddr::new(0x8000));
        assert_eq!(pmm.frame_metadata(first).unwrap().owner_tag, 0x5749434f);
        assert_eq!(pmm.allocated_frames(), 3);
        assert_eq!(pmm.free_frames(), 1);
    }

    #[test]
    fn frees_frames_and_reuses_them_without_double_allocation() {
        let ranges = [range(0x1000, 0x3000)];
        let mut bitmap = [0u8; 1];
        let mut metadata = [FrameMetadata::default(); 3];
        let mut pmm = Pmm::new(&ranges, PAGE, &mut bitmap, &mut metadata).unwrap();

        let first = pmm.allocate_frame(1).unwrap();
        let second = pmm.allocate_frame(2).unwrap();
        pmm.free_frame(first).unwrap();
        assert_eq!(pmm.free_frames(), 2);
        assert_eq!(pmm.frame_metadata(first).unwrap(), FrameMetadata::default());
        assert_eq!(pmm.allocate_frame(3).unwrap(), first);
        assert_eq!(pmm.allocated_frames(), 2);
        assert_eq!(pmm.frame_metadata(second).unwrap().owner_tag, 2);
    }

    #[test]
    fn reports_exhaustion_and_rejects_invalid_free_operations() {
        let ranges = [range(0x1000, 0x1000)];
        let mut bitmap = [0u8; 1];
        let mut metadata = [FrameMetadata::default(); 1];
        let mut pmm = Pmm::new(&ranges, PAGE, &mut bitmap, &mut metadata).unwrap();
        let frame = pmm.allocate_frame(1).unwrap();
        assert_eq!(pmm.allocate_frame(2), Err(PmmError::OutOfMemory));
        assert_eq!(pmm.free_frame(PhysAddr::new(0x1001)), Err(PmmError::UnalignedAddress));
        assert_eq!(pmm.free_frame(PhysAddr::new(0x9000)), Err(PmmError::UntrackedFrame));
        pmm.free_frame(frame).unwrap();
        assert_eq!(pmm.free_frame(frame), Err(PmmError::DoubleFree));
    }

    #[test]
    fn rejects_invalid_ranges_and_insufficient_storage_transactionally() {
        let ranges = [range(0x3000, 0x1000), range(0x1000, 0x1000)];
        let mut bitmap = [0xa5u8; 1];
        let mut metadata = [FrameMetadata { owner_tag: 9, flags: 1, allocation_id: 2 }; 2];
        assert_eq!(
            Pmm::new(&ranges, PAGE, &mut bitmap, &mut metadata).err(),
            Some(PmmError::InvalidRanges)
        );
        assert_eq!(bitmap, [0xa5]);
        assert_eq!(metadata[0].owner_tag, 9);

        let valid = [range(0x1000, 0x2000)];
        let mut short_bitmap = [0x5a];
        let mut enough_metadata = [FrameMetadata::default(); 2];
        assert_eq!(
            Pmm::new(&valid, PAGE, &mut short_bitmap[..0], &mut enough_metadata).err(),
            Some(PmmError::InsufficientBitmap { required: 1, available: 0 })
        );
        assert_eq!(short_bitmap, [0x5a]);
    }

    #[test]
    fn rejects_overlap_unaligned_and_overflowing_ranges() {
        let mut bitmap = [0u8; 2];
        let mut metadata = [FrameMetadata::default(); 8];
        assert_eq!(
            Pmm::new(
                &[range(0x1000, 0x2000), range(0x2000, 0x1000)],
                PAGE,
                &mut bitmap,
                &mut metadata
            ).err(),
            Some(PmmError::InvalidRanges)
        );
        assert_eq!(
            Pmm::new(&[range(0x1001, 0x1000)], PAGE, &mut bitmap, &mut metadata).err(),
            Some(PmmError::InvalidRanges)
        );
        assert_eq!(
            Pmm::new(&[range(u64::MAX - 0xfff, 0x1000)], PAGE, &mut bitmap, &mut metadata).err(),
            Some(PmmError::ArithmeticOverflow)
        );
    }
}
