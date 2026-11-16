#![no_std]
#![no_main]

//! Freestanding kernel image entry point.

extern crate wicofe_kernel;

use core::fmt::{self, Write};
use core::panic::PanicInfo;

// Keep the library object containing the Limine bootstrap in the final link.
#[used]
static KEEP_BOOTSTRAP: extern "C" fn() -> ! = wicofe_kernel::boot::wicofe_bootstrap;

struct EarlyConsole;

#[cfg(target_arch = "x86_64")]
impl EarlyConsole {
    fn out(port: u16, value: u8) {
        // SAFETY: Port-mapped UART access is valid in the ring-0 kernel context.
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") port,
                in("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }

    fn initialize() {
        Self::out(0x3f9, 0x00);
        Self::out(0x3fb, 0x80);
        Self::out(0x3f8, 0x01);
        Self::out(0x3f9, 0x00);
        Self::out(0x3fb, 0x03);
        Self::out(0x3fa, 0xc7);
        Self::out(0x3fc, 0x0b);
    }

    fn write_byte(byte: u8) {
        // Bound the wait so a missing UART cannot deadlock panic handling.
        for _ in 0..100_000 {
            let status: u8;
            // SAFETY: Reading the 16550 line-status register at COM1.
            unsafe {
                core::arch::asm!(
                    "in al, dx",
                    in("dx") 0x3fd_u16,
                    out("al") status,
                    options(nomem, nostack, preserves_flags)
                );
            }
            if status & 0x20 != 0 {
                Self::out(0x3f8, byte);
                return;
            }
            core::hint::spin_loop();
        }
    }
}

impl Write for EarlyConsole {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        #[cfg(target_arch = "x86_64")]
        for byte in value.bytes() {
            if byte == b'\n' {
                Self::write_byte(b'\r');
            }
            Self::write_byte(byte);
        }

        #[cfg(target_arch = "aarch64")]
        let _ = value;

        Ok(())
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    #[cfg(target_arch = "x86_64")]
    EarlyConsole::initialize();

    let _ = writeln!(EarlyConsole, "WICOFE PANIC: {info}");

    loop {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: Panic handling is terminal; interrupts remain disabled.
        unsafe {
            core::arch::asm!("cli; hlt", options(nomem, nostack));
        }

        #[cfg(target_arch = "aarch64")]
        // SAFETY: Panic handling is terminal; wait for an event indefinitely.
        unsafe {
            core::arch::asm!("msr daifset, #0xf; wfe", options(nomem, nostack));
        }
    }
}
