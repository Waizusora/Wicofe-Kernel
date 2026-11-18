//! Common syscall result encoding for the adopted Wicofe carry-flag ABI.
//!
//! This module models the result that architecture return code must place in
//! the user register frame. It does not install syscall entry stubs or perform
//! the final x86-64 SYSRETQ / AArch64 ERET transition.

use core::num::NonZeroU32;

/// Architecture-neutral request passed from an entry adapter to the dispatcher.
///
/// Architecture code is responsible for decoding the syscall number and six
/// argument registers into this structure. The numeric syscall namespace is
/// intentionally not assigned by this kernel module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyscallRequest {
    /// ABI-defined syscall number. Values are not yet allocated.
    pub number: usize,
    /// Arguments in ABI order, independent of the source CPU registers.
    pub arguments: [usize; 6],
}

impl SyscallRequest {
    /// Construct a request after architecture-specific register decoding.
    pub const fn new(number: usize, arguments: [usize; 6]) -> Self {
        Self { number, arguments }
    }
}

/// Kernel policy implementation for a syscall namespace.
///
/// Implementations must validate user pointers and lengths before access,
/// must not retain borrowed user addresses beyond the operation, and must
/// return a positive Wicofe errno for failures. This trait does not define
/// syscall numbers or authorize user memory access by itself.
pub trait SyscallDispatcher {
    /// Dispatch one decoded request.
    fn dispatch(&mut self, request: SyscallRequest) -> SyscallOutcome;
}

/// Dispatch a request and encode its architecture-neutral return value.
pub fn dispatch_request(
    dispatcher: &mut impl SyscallDispatcher,
    request: SyscallRequest,
) -> SyscallReturn {
    SyscallReturn::from_outcome(dispatcher.dispatch(request))
}

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
    use super::{
        dispatch_request, SyscallDispatcher, SyscallOutcome, SyscallRequest, SyscallReturn,
    };
    use core::num::NonZeroU32;

    struct TestDispatcher;

    impl SyscallDispatcher for TestDispatcher {
        fn dispatch(&mut self, request: SyscallRequest) -> SyscallOutcome {
            assert_eq!(request.number, 7);
            assert_eq!(request.arguments, [1, 2, 3, 4, 5, 6]);
            SyscallOutcome::Success(42)
        }
    }

    #[test]
    fn request_preserves_number_and_all_six_arguments() {
        let request = SyscallRequest::new(7, [1, 2, 3, 4, 5, 6]);
        assert_eq!(request.number, 7);
        assert_eq!(request.arguments, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn dispatch_request_encodes_success() {
        let mut dispatcher = TestDispatcher;
        let request = SyscallRequest::new(7, [1, 2, 3, 4, 5, 6]);
        assert_eq!(
            dispatch_request(&mut dispatcher, request),
            SyscallReturn { value: 42, carry: false }
        );
    }

    struct ErrorDispatcher;

    impl SyscallDispatcher for ErrorDispatcher {
        fn dispatch(&mut self, _request: SyscallRequest) -> SyscallOutcome {
            SyscallOutcome::Error(NonZeroU32::new(36).expect("ENOSYS is non-zero"))
        }
    }

    #[test]
    fn dispatch_request_encodes_errno_as_positive_carry_error() {
        let mut dispatcher = ErrorDispatcher;
        let request = SyscallRequest::new(999, [0; 6]);
        assert_eq!(
            dispatch_request(&mut dispatcher, request),
            SyscallReturn { value: 36, carry: true }
        );
    }

    #[test]
    fn zero_success_clears_carry() {
        assert_eq!(
            SyscallReturn::from_outcome(SyscallOutcome::Success(0)),
            SyscallReturn { value: 0, carry: false }
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
