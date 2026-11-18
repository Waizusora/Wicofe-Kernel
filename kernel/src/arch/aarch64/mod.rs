//! AArch64 architecture backend.
//!
//! The Limine entry trampoline masks interrupts and transfers to the common
//! Rust bootstrap using the bootloader-provided 64 KiB stack. It deliberately
//! leaves EL selection and translation-table ownership unchanged in this
//! milestone.

/// Architecture identifier used by early diagnostics.
pub mod user_context;

pub const ARCH_NAME: &str = "aarch64";

#[cfg(target_os = "none")]
core::arch::global_asm!(
    r#"
    .section .text.entry,"ax",%progbits
    .balign 16
    .global _start
    .type _start,%function
_start:
    msr daifset, #0xf
    bl wicofe_bootstrap
1:
    wfe
    b 1b
    .size _start, . - _start
"#
);
