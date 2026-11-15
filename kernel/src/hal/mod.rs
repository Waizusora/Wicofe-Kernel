//! Hardware abstraction layer boundary.
//!
//! Architecture-specific implementations are selected at compile time. This
//! module deliberately exposes no stable cross-DKM ABI.

#[cfg(target_arch = "x86_64")]
pub use crate::arch::x86_64 as current_arch;

#[cfg(target_arch = "aarch64")]
pub use crate::arch::aarch64 as current_arch;
