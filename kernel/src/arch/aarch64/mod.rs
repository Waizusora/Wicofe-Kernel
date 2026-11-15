//! AArch64 architecture backend.
//!
//! Limine entry assembly, exception-level setup, exception vectors, GIC, and
//! translation-table operations will be added in dedicated implementation steps.

/// Architecture identifier used by early diagnostics.
pub const ARCH_NAME: &str = "aarch64";
