# x86-64 Kernel Image Build

This milestone builds a freestanding x86-64 ELF kernel image. It does not yet produce a Limine bootable disk/ISO image.

## Build

From the repository root, with the Rust target installed:

```sh
rustup target add x86_64-unknown-none
cargo build -p wicofe-kernel --features kernel-image --bin wicofe-kernel --target x86_64-unknown-none
```

The `kernel-image` feature explicitly enables the freestanding binary while keeping it out of host `cargo test --workspace`. The target-specific Cargo configuration applies the x86-64 kernel code model, static relocation, disabled red zone, panic-abort behavior, and `linker/x86_64.ld`. The output is `target/x86_64-unknown-none/debug/wicofe-kernel`.

## ELF layout validation

Inspect the linked image:

```sh
readelf -h target/x86_64-unknown-none/debug/wicofe-kernel
readelf -l target/x86_64-unknown-none/debug/wicofe-kernel
python3 scripts/verify_kernel_elf.py target/x86_64-unknown-none/debug/wicofe-kernel
```

The verifier checks ELF64 little-endian format, x86-64 `ET_EXEC`, the higher-half entry point, exactly three loadable segments, 4 KiB alignment, the VMA/LMA offset, ordered physical load addresses, valid file/memory sizes, and non-writable-executable segment permissions. CI runs this verifier on every push and pull request.

The image provides the `_start` entry trampoline through the architecture backend and a panic handler in the binary crate. On x86-64, panic output is sent to COM1 (I/O port 0x3F8) with a bounded transmitter wait; if the UART is unavailable, panic handling still terminates in a halt loop.

## Current limits

- The image is an ELF kernel executable, not a Limine bootable ISO or disk image.
- QEMU boot and serial-output assertions are not yet part of CI.
- The bootstrap currently initializes the early PMM and then deliberately holds it; general allocation remains disabled.
- No IDT, exception handlers, VMM, SMP synchronization, or framebuffer console is initialized.
- AArch64 remains covered by target compilation, but its final ELF linker integration and runtime boot test are separate work.

The next step is to package this ELF with a pinned Limine release and boot it under QEMU, asserting a deterministic serial marker before enabling more kernel subsystems.
