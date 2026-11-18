//! Common syscall result encoding for the adopted Wicofe carry-flag ABI.
//!
//! This module models the result that architecture return code must place in
//! the user register frame. It does not install syscall entry stubs or perform
//! the final x86-64 SYSRETQ / AArch64 ERET transition.

use core::num::NonZeroU32;

/// Logical outcome of a dispatched syscall.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallOutcome {
    /// Successful operation. The full machine-word result is preserved.
    Success(usize),
    /// Failed operation. The value is a positive Wicofe errno.
    Error(NonZeroU32),
}

/// Machine-word result plus the carry discriminator for architecture return.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyscallReturn {
    /// Value to place in RAX (x86-64) or X0 (AArch64).
    pub value: usize,
    /// Set RFLAGS.CF / PSTATE.C when true.
    pub carry: bool,
}

impl SyscallReturn {
    /// Encode a successful syscall result.
    pub const fn success(value: usize) -> Self {
        Self {
            value,
            carry: false,
        }
    }

    /// Encode a successful signed result without reserving negative values.
    pub const fn success_signed(value: isize) -> Self {
        Self::success(value as usize)
    }

    /// Encode a failed syscall with a positive, non-zero Wicofe errno.
    pub const fn failure(errno: NonZeroU32) -> Self {
        Self {
            value: errno.get() as usize,
            carry: true,
        }
    }

    /// Convert a logical syscall outcome to the architecture-neutral return.
    pub const fn from_outcome(outcome: SyscallOutcome) -> Self {
        match outcome {
            SyscallOutcome::Success(value) => Self::success(value),
            SyscallOutcome::Error(errno) => Self::failure(errno),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SyscallOutcome, SyscallReturn};
    use core::num::NonZeroU32;

    #[test]
    fn zero_success_clears_carry() {
        assert_eq!(
            SyscallReturn::from_outcome(SyscallOutcome::Success(0)),
            SyscallReturn {
                value: 0,
                carry: false,
            }
        );
    }

    #[test]
    fn positive_success_clears_carry() {
        let result = SyscallReturn::success(123);
        assert_eq!(result.value, 123);
        assert!(!result.carry);
    }

    #[test]
    fn negative_signed_success_is_not_an_error() {
        let result = SyscallReturn::success_signed(-1);
        assert_eq!(result.value, (-1_isize) as usize);
        assert!(!result.carry);
    }

    #[test]
    fn failure_returns_positive_errno_and_sets_carry() {
        let errno = NonZeroU32::new(2).expect("non-zero errno");
        let result = SyscallReturn::from_outcome(SyscallOutcome::Error(errno));
        assert_eq!(result.value, 2);
        assert!(result.carry);
    }
}
