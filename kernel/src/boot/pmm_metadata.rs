//! Bounded placement planning for physical-memory-manager metadata.
//!
//! This module computes metadata capacity from the early usable-memory plan,
//! chooses one contiguous backing range, and removes that range from the
//! ranges that may later be handed to the PMM. It does not write metadata,
//! initialize allocator state, or transfer ownership.

use core::num::NonZeroU64;

use super::EarlyMemoryRange;

/// Physical layout and capacity requirements for PMM metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PmmMetadataLayout {
    /// Physical base of the metadata backing allocation.
    pub physical_base: u64,
    /// Page-rounded physical length reserved for metadata.
    pub reserved_length: u64,
    /// Number of page frames represented by the metadata.
    pub tracked_frames: u64,
    /// Bitmap capacity in bytes, rounded up to a whole byte.
    pub bitmap_bytes: u64,
    /// Byte offset of the per-frame metadata array from `physical_base`.
    /// This is aligned for `FrameMetadata` (8 bytes).
    pub frame_metadata_offset: u64,
    /// Per-frame metadata capacity in bytes.
    pub frame_metadata_bytes: u64,
}

/// Errors while planning PMM metadata placement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmmMetadataError {
    InvalidPageSize,
    InvalidFrameMetadataSize,
    InvalidEarlyMemoryPlan,
    ArithmeticOverflow,
    InsufficientBackingMemory,
    InsufficientOutputCapacity { required: usize, available: usize },
}

/// Calculate metadata capacity, select first-fit backing storage, and return
/// candidate ranges with the metadata backing range removed.
///
/// Input ranges must be sorted by physical base, page-aligned, non-empty, and
/// non-overlapping. Adjacent ranges are accepted but remain distinct. The
/// bitmap uses one bit per tracked frame; frame_metadata_size specifies the
/// caller-selected bytes per frame. All size calculations are checked.
///
/// The metadata backing range is selected wholly inside one input range and
/// starts at that range's base. The output contains every remaining candidate
/// range, sorted and page-aligned. The counting pass guarantees that an
/// insufficient output slice is left unchanged. No memory is initialized or
/// made allocatable by this function.
pub fn plan_pmm_metadata_placement(
    ranges: &[EarlyMemoryRange],
    page_size: NonZeroU64,
    frame_metadata_size: NonZeroU64,
    output: &mut [EarlyMemoryRange],
) -> Result<(PmmMetadataLayout, usize), PmmMetadataError> {
    let page = page_size.get();
    if !page.is_power_of_two() {
        return Err(PmmMetadataError::InvalidPageSize);
    }
    if frame_metadata_size.get() == 0 {
        return Err(PmmMetadataError::InvalidFrameMetadataSize);
    }

    let mut total_frames = 0u64;
    let mut previous_end = None;
    for range in ranges {
        if range.length == 0 || range.base % page != 0 || range.length % page != 0 {
            return Err(PmmMetadataError::InvalidEarlyMemoryPlan);
        }
        let end = range
            .base
            .checked_add(range.length)
            .ok_or(PmmMetadataError::ArithmeticOverflow)?;
        if previous_end.is_some_and(|previous| range.base < previous) {
            return Err(PmmMetadataError::InvalidEarlyMemoryPlan);
        }
        previous_end = Some(end);
        total_frames = total_frames
            .checked_add(range.length / page)
            .ok_or(PmmMetadataError::ArithmeticOverflow)?;
    }
    if total_frames == 0 {
        return Err(PmmMetadataError::InsufficientBackingMemory);
    }

    let bitmap_bytes = total_frames
        .checked_add(7)
        .ok_or(PmmMetadataError::ArithmeticOverflow)?
        / 8;
    let frame_metadata_bytes = total_frames
        .checked_mul(frame_metadata_size.get())
        .ok_or(PmmMetadataError::ArithmeticOverflow)?;
    // FrameMetadata is repr(C) with 8-byte alignment. Keep the physical
    // metadata start suitably aligned even when bitmap_bytes is not.
    let frame_metadata_offset =
        align_up(bitmap_bytes, 8).ok_or(PmmMetadataError::ArithmeticOverflow)?;
    let raw_bytes = frame_metadata_offset
        .checked_add(frame_metadata_bytes)
        .ok_or(PmmMetadataError::ArithmeticOverflow)?;
    let reserved_length = align_up(raw_bytes, page).ok_or(PmmMetadataError::ArithmeticOverflow)?;

    let mut placement = None;
    for range in ranges {
        if range.length >= reserved_length {
            placement = Some((range.base, range.length));
            break;
        }
    }
    let (physical_base, source_length) =
        placement.ok_or(PmmMetadataError::InsufficientBackingMemory)?;
    let source_end = physical_base
        .checked_add(source_length)
        .ok_or(PmmMetadataError::ArithmeticOverflow)?;
    let metadata_end = physical_base
        .checked_add(reserved_length)
        .ok_or(PmmMetadataError::ArithmeticOverflow)?;

    let suffix_length = source_end - metadata_end;
    let extra = usize::from(suffix_length != 0);
    let required = ranges
        .len()
        .checked_sub(1)
        .and_then(|count| count.checked_add(extra))
        .ok_or(PmmMetadataError::ArithmeticOverflow)?;
    if output.len() < required {
        return Err(PmmMetadataError::InsufficientOutputCapacity {
            required,
            available: output.len(),
        });
    }

    let mut written = 0usize;
    for range in ranges {
        if range.base == physical_base && range.length == source_length {
            if suffix_length != 0 {
                output[written] = EarlyMemoryRange {
                    base: metadata_end,
                    length: suffix_length,
                };
                written += 1;
            }
        } else {
            output[written] = *range;
            written += 1;
        }
    }

    Ok((
        PmmMetadataLayout {
            physical_base,
            reserved_length,
            tracked_frames: total_frames,
            bitmap_bytes,
            frame_metadata_offset,
            frame_metadata_bytes,
        },
        written,
    ))
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let remainder = value % alignment;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(alignment - remainder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: NonZeroU64 = match NonZeroU64::new(0x1000) {
        Some(value) => value,
        None => panic!("page size must be non-zero"),
    };
    const FRAME_META: NonZeroU64 = match NonZeroU64::new(16) {
        Some(value) => value,
        None => panic!("metadata size must be non-zero"),
    };

    const fn range(base: u64, length: u64) -> EarlyMemoryRange {
        EarlyMemoryRange { base, length }
    }

    #[test]
    fn places_metadata_and_removes_backing_from_candidates() {
        let ranges = [range(0x1000, 0x20_000), range(0x40_000, 0x10_000)];
        let mut output = [range(0, 0); 2];
        let (layout, count) =
            plan_pmm_metadata_placement(&ranges, PAGE, FRAME_META, &mut output).unwrap();

        assert_eq!(layout.tracked_frames, 48);
        assert_eq!(layout.bitmap_bytes, 6);
        assert_eq!(layout.frame_metadata_bytes, 48 * 16);
        assert_eq!(layout.physical_base, 0x1000);
        assert_eq!(layout.reserved_length, 0x1000);
        assert_eq!(count, 2);
        assert_eq!(output[0], range(0x2000, 0x1f_000));
        assert_eq!(output[1], ranges[1]);
    }

    #[test]
    fn preserves_ranges_before_selected_backing() {
        let ranges = [range(0x1000, 0x1000), range(0x4000, 0x20_000)];
        let mut output = [range(0, 0); 2];
        let large_metadata = NonZeroU64::new(128).unwrap();
        let (layout, count) =
            plan_pmm_metadata_placement(&ranges, PAGE, large_metadata, &mut output).unwrap();
        assert_eq!(layout.physical_base, 0x4000);
        assert_eq!(count, 2);
        assert_eq!(output[0], range(0x1000, 0x1000));
        assert_eq!(output[1], range(0x6000, 0x1e_000));
    }

    #[test]
    fn insufficient_output_capacity_is_transactional() {
        let ranges = [range(0x1000, 0x20_000), range(0x40_000, 0x1000)];
        let sentinel = range(7, 9);
        let mut output = [sentinel; 1];
        assert_eq!(
            plan_pmm_metadata_placement(&ranges, PAGE, FRAME_META, &mut output),
            Err(PmmMetadataError::InsufficientOutputCapacity {
                required: 2,
                available: 1
            })
        );
        assert_eq!(output, [sentinel]);
    }

    #[test]
    fn rejects_unsorted_overlapping_and_unaligned_ranges() {
        let mut output = [range(0, 0); 4];
        assert_eq!(
            plan_pmm_metadata_placement(
                &[range(0x3000, 0x1000), range(0x1000, 0x1000)],
                PAGE,
                FRAME_META,
                &mut output
            ),
            Err(PmmMetadataError::InvalidEarlyMemoryPlan)
        );
        assert_eq!(
            plan_pmm_metadata_placement(
                &[range(0x1000, 0x3000), range(0x3000, 0x1000)],
                PAGE,
                FRAME_META,
                &mut output
            ),
            Err(PmmMetadataError::InvalidEarlyMemoryPlan)
        );
        assert_eq!(
            plan_pmm_metadata_placement(&[range(0x1001, 0x2000)], PAGE, FRAME_META, &mut output),
            Err(PmmMetadataError::InvalidEarlyMemoryPlan)
        );
    }

    #[test]
    fn reports_insufficient_contiguous_backing() {
        let ranges = [range(0x1000, 0x1000), range(0x4000, 0x1000)];
        let mut output = [range(0, 0); 2];
        let large_metadata = NonZeroU64::new(0x3000).unwrap();
        assert_eq!(
            plan_pmm_metadata_placement(&ranges, PAGE, large_metadata, &mut output),
            Err(PmmMetadataError::InsufficientBackingMemory)
        );
    }

    #[test]
    fn rejects_arithmetic_overflow() {
        let ranges = [range(u64::MAX - 0xfff, 0x1000)];
        let mut output = [range(0, 0); 1];
        assert_eq!(
            plan_pmm_metadata_placement(&ranges, PAGE, FRAME_META, &mut output),
            Err(PmmMetadataError::ArithmeticOverflow)
        );
    }

    #[test]
    fn validates_page_and_frame_metadata_sizes() {
        let ranges = [range(0x1000, 0x20_000)];
        let mut output = [range(0, 0); 2];
        assert_eq!(
            plan_pmm_metadata_placement(
                &ranges,
                NonZeroU64::new(3000).unwrap(),
                FRAME_META,
                &mut output
            ),
            Err(PmmMetadataError::InvalidPageSize)
        );
    }
}
