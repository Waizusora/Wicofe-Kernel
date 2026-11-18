//! Complete software-owned EL0 register snapshot for AArch64.
//!
//! This is a layout contract for future vector/return code, not a hardware
//! exception frame. The EL1 vector entry must save the required registers
//! explicitly and preserve the saved SPSR_EL1.C state through return.

/// Full EL0 execution context required to resume an AArch64 thread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct UserContext {
    /// General-purpose registers X0 through X30, in register-number order.
    pub x: [u64; 31],
    /// User stack pointer captured from SP_EL0.
    pub sp_el0: u64,
    /// User instruction pointer captured from ELR_EL1.
    pub elr_el1: u64,
    /// Saved user processor state captured from SPSR_EL1.
    pub spsr_el1: u64,
    /// User thread pointer captured from TPIDR_EL0.
    pub tpidr_el0: u64,
}

impl Default for UserContext {
    fn default() -> Self {
        Self {
            x: [0; 31],
            sp_el0: 0,
            elr_el1: 0,
            spsr_el1: 0,
            tpidr_el0: 0,
        }
    }
}

impl UserContext {
    pub const GPR_COUNT: usize = 31;
    pub const REGISTER_COUNT: usize = 35;

    /// Return whether the saved SPSR mode denotes EL0t.
    /// This does not validate the user address space or instruction pointer.
    pub const fn returns_to_el0t(&self) -> bool {
        (self.spsr_el1 & 0xf) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::UserContext;
    use core::mem::{offset_of, size_of};

    #[test]
    fn context_layout_contains_all_user_return_state() {
        assert_eq!(size_of::<UserContext>(), 35 * size_of::<u64>());
        assert_eq!(offset_of!(UserContext, x), 0);
        assert_eq!(offset_of!(UserContext, sp_el0), 31 * 8);
        assert_eq!(offset_of!(UserContext, elr_el1), 32 * 8);
        assert_eq!(offset_of!(UserContext, spsr_el1), 33 * 8);
        assert_eq!(offset_of!(UserContext, tpidr_el0), 34 * 8);
    }

    #[test]
    fn context_identifies_el0t_return_mode() {
        let mut context = UserContext {
            spsr_el1: 0,
            ..UserContext::default()
        };
        assert!(context.returns_to_el0t());
        context.spsr_el1 = 0b0101;
        assert!(!context.returns_to_el0t());
    }
}
