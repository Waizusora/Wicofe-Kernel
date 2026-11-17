//! x86-64 architecture backend.
//!
//! The Limine entry trampoline uses the bootloader-provided 64 KiB stack,
//! clears the direction flag, aligns the stack for the System V ABI, and
//! transfers to the common Rust bootstrap. It does not replace the bootloader
//! page tables or install descriptor tables yet.

pub mod gdt;
pub mod interrupts;
pub mod paging;
pub mod pic;

/// Architecture identifier used by early diagnostics.
pub const ARCH_NAME: &str = "x86_64";

#[cfg(target_os = "none")]
core::arch::global_asm!(
    r#"
    .section .text.entry,"ax",@progbits
    .balign 16
    .global _start
    .type _start,@function
_start:
    cli
    cld
    andq $-16, %rsp
    call wicofe_bootstrap
1:
    cli
    hlt
    jmp 1b
    .size _start, . - _start
"#,
    options(att_syntax)
);
