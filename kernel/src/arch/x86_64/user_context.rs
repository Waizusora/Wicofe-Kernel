//! Complete software-owned user register snapshot for x86-64.
//!
//! This is a layout contract for future entry/return code, not a hardware
//! interrupt frame. SYSCALL does not push a frame; entry assembly must save
//! these values explicitly before using the registers as scratch state.

/// Full user execution context required to resume a ring-3 thread.
///
/// GPR fields are ordered to make assembly offsets explicit. The final five
/// fields are the architectural return frame and user TLS base snapshots.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct UserContext {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
    pub fs_base: u64,
    pub gs_base: u64,
}

impl UserContext {
    pub const GPR_COUNT: usize = 15;
    pub const REGISTER_COUNT: usize = 22;

    /// Return whether the context carries the mandatory ring-3 privilege bits.
    /// This is a structural check only; it does not validate mapped addresses.
    pub const fn has_user_privilege_selectors(&self) -> bool {
        (self.cs & 3) == 3 && (self.ss & 3) == 3
    }
}

#[cfg(test)]
mod tests {
    use super::UserContext;
    use core::mem::{offset_of, size_of};

    #[test]
    fn context_layout_is_stable_and_complete() {
        assert_eq!(size_of::<UserContext>(), 22 * size_of::<u64>());
        assert_eq!(offset_of!(UserContext, r15), 0);
        assert_eq!(offset_of!(UserContext, rax), 14 * 8);
        assert_eq!(offset_of!(UserContext, rip), 15 * 8);
        assert_eq!(offset_of!(UserContext, rflags), 17 * 8);
        assert_eq!(offset_of!(UserContext, rsp), 18 * 8);
        assert_eq!(offset_of!(UserContext, ss), 19 * 8);
        assert_eq!(offset_of!(UserContext, fs_base), 20 * 8);
        assert_eq!(offset_of!(UserContext, gs_base), 21 * 8);
    }

    #[test]
    fn user_selector_check_requires_ring_three() {
        let mut context = UserContext { cs: 0x2b, ss: 0x23, ..UserContext::default() };
        assert!(context.has_user_privilege_selectors());
        context.cs = 0x08;
        assert!(!context.has_user_privilege_selectors());
    }
}
