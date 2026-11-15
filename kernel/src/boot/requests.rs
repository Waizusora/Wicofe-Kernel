//! Static Limine request records for the WICOFE boot handoff.
//!
//! The request records are separate from response decoding. Limine patches
//! each response pointer before transferring control to the executable. The
//! linker must retain this section in a writable PT_LOAD segment.

use core::cell::UnsafeCell;
use core::ffi::c_void;

/// Limine common request magic.
pub const LIMINE_COMMON_MAGIC: [u64; 2] = [
    0xc7b1_dd30_df4c_8b88,
    0x0a82_e883_a194_f07b,
];

/// Base revision requested by this kernel.
pub const LIMINE_BASE_REVISION: u64 = 6;

const REQUESTS_START: [u64; 4] = [
    0xf6b8_f4b3_9de7_d1ae,
    0xfab9_1a69_40fc_b9cf,
    0x785c_6ed0_15d3_e316,
    0x181e_920a_7852_b9d9,
];

const REQUESTS_END: [u64; 2] = [
    0xadc0_e053_1bb1_0d03,
    0x9572_709f_3176_4c62,
];

/// Limine base-revision tag. The bootloader writes zero to revision when the
/// requested base revision is supported.
#[repr(C)]
pub struct BaseRevisionTag {
    pub magic: [u64; 2],
    pub revision: UnsafeCell<u64>,
}

// SAFETY: The object is a protocol record shared with the bootloader. The
// revision cell is the only field the bootloader is permitted to update.
unsafe impl Sync for BaseRevisionTag {}

/// Common request ABI prefix used by the selected Limine features.
#[repr(C)]
pub struct LimineRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: UnsafeCell<*mut c_void>,
}

// SAFETY: The response pointer is intentionally interior-mutable because the
// bootloader fills it before entry. The other fields are immutable.
unsafe impl Sync for LimineRequest {}

/// Request ABI for Limine's stack-size feature.
#[repr(C)]
pub struct LimineStackSizeRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: UnsafeCell<*mut c_void>,
    pub stack_size: u64,
}

// SAFETY: The response pointer is intentionally interior-mutable.
unsafe impl Sync for LimineStackSizeRequest {}

#[used]
#[unsafe(link_section = ".limine_requests_start")]
static LIMINE_REQUESTS_START: [u64; 4] = REQUESTS_START;

#[used]
#[unsafe(link_section = ".limine_requests")]
pub static LIMINE_BASE_REVISION_TAG: BaseRevisionTag = BaseRevisionTag {
    magic: [0xf956_2b2d_5c95_a6c8, 0x6a7b_3849_4453_6bdc],
    revision: UnsafeCell::new(LIMINE_BASE_REVISION),
};

#[used]
#[unsafe(link_section = ".limine_requests")]
pub static LIMINE_MEMMAP_REQUEST: LimineRequest = LimineRequest {
    id: [LIMINE_COMMON_MAGIC[0], LIMINE_COMMON_MAGIC[1], 0x67cf_3d9d_378a_806f, 0xe304_acdf_c50c_3c62],
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[unsafe(link_section = ".limine_requests")]
pub static LIMINE_HHDM_REQUEST: LimineRequest = LimineRequest {
    id: [LIMINE_COMMON_MAGIC[0], LIMINE_COMMON_MAGIC[1], 0x48dc_f1cb_8ad2_b852, 0x6398_4e95_9a98_244b],
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[unsafe(link_section = ".limine_requests")]
pub static LIMINE_EXECUTABLE_ADDRESS_REQUEST: LimineRequest = LimineRequest {
    id: [LIMINE_COMMON_MAGIC[0], LIMINE_COMMON_MAGIC[1], 0x71ba_7686_3cc5_5f63, 0xb264_4a48_c516_a487],
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[unsafe(link_section = ".limine_requests")]
pub static LIMINE_MODULE_REQUEST: LimineRequest = LimineRequest {
    id: [LIMINE_COMMON_MAGIC[0], LIMINE_COMMON_MAGIC[1], 0x3e7e_2797_02be_32af, 0xca1c_4f3b_d128_0cee],
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[unsafe(link_section = ".limine_requests")]
pub static LIMINE_FRAMEBUFFER_REQUEST: LimineRequest = LimineRequest {
    id: [LIMINE_COMMON_MAGIC[0], LIMINE_COMMON_MAGIC[1], 0x9d58_27dc_d881_dd75, 0xa314_8604_f6fa_b11b],
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[unsafe(link_section = ".limine_requests")]
pub static LIMINE_STACK_SIZE_REQUEST: LimineStackSizeRequest = LimineStackSizeRequest {
    id: [LIMINE_COMMON_MAGIC[0], LIMINE_COMMON_MAGIC[1], 0x224e_f046_0a8e_8926, 0xe1cb_0fc2_5f46_ea3d],
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
    stack_size: 64 * 1024,
};

#[used]
#[unsafe(link_section = ".limine_requests_end")]
static LIMINE_REQUESTS_END: [u64; 2] = REQUESTS_END;

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn request_records_match_common_abi_layout() {
        assert_eq!(size_of::<LimineRequest>(), 48);
        assert_eq!(align_of::<LimineRequest>(), 8);
        assert_eq!(size_of::<LimineStackSizeRequest>(), 56);
        assert_eq!(align_of::<LimineStackSizeRequest>(), 8);
    }

    #[test]
    fn base_revision_tag_matches_protocol_layout() {
        assert_eq!(size_of::<BaseRevisionTag>(), 24);
        assert_eq!(align_of::<BaseRevisionTag>(), 8);
        assert_eq!(LIMINE_BASE_REVISION, 6);
    }

    #[test]
    fn requested_ids_use_common_magic() {
        for request in [
            &LIMINE_MEMMAP_REQUEST,
            &LIMINE_HHDM_REQUEST,
            &LIMINE_EXECUTABLE_ADDRESS_REQUEST,
            &LIMINE_MODULE_REQUEST,
            &LIMINE_FRAMEBUFFER_REQUEST,
        ] {
            assert_eq!(&request.id[..2], &LIMINE_COMMON_MAGIC);
            assert_eq!(request.revision, 0);
        }
        assert_eq!(&LIMINE_STACK_SIZE_REQUEST.id[..2], &LIMINE_COMMON_MAGIC);
    }
}
