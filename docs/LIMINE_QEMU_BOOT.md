# Limine x86-64 Boot Image and QEMU Validation

WICOFE uses a pinned Limine binary release to package the x86-64 ELF kernel into a BIOS/UEFI hybrid ISO. The bootloader is a build-time third-party tool; it is not linked into the kernel.

## Pinned bootloader

- Project: [Limine](https://github.com/Limine-Bootloader/Limine)
- Release: `v12.9.6`
- Binary archive: `limine-binary.tar.xz`
- SHA-256: `939f670e0831cc940453fd3a0ddfb2b324c0b4e3c5d0e741c190aa0d13df0103`
- Upstream project license: BSD-2-Clause

The ISO builder downloads the exact release archive and verifies its SHA-256 before extraction. The upstream usage guide documents the hybrid ISO layout and the required BIOS installation step.

## Build

Install the host tools and Rust target:

```sh
sudo apt install build-essential xorriso qemu-system-x86
rustup target add x86_64-unknown-none
```

Build the kernel ELF and ISO:

```sh
cargo build -p wicofe-kernel --features kernel-image --bin wicofe-kernel --target x86_64-unknown-none
scripts/build_limine_iso.sh target/wicofe-x86_64.iso
```

Boot it under QEMU:

```sh
timeout 20s qemu-system-x86_64 \
  -machine q35 -m 512M -cdrom target/wicofe-x86_64.iso -boot d \
  -display none -serial file:target/qemu-serial.log \
  -no-reboot -no-shutdown
cat target/qemu-serial.log
```

A successful early boot prints `WICOFE: BOOTSTRAP_ENTERED`, `WICOFE: BOOTINFO_VALID`, and `WICOFE: PMM_READY` to COM1. CI requires the PMM marker and rejects bootstrap failure markers.

## Runtime test scope

The QEMU test boots the Limine ISO using the emulated x86-64 BIOS path. It confirms that Limine loads the kernel, the kernel consumes and validates the boot handoff, and the initial HHDM-backed PMM is initialized. The kernel intentionally remains in a terminal loop after PMM initialization.

This is not yet a full OS boot. There is no VMM, interrupt/exception subsystem, SMP startup, general heap allocator, userspace, or filesystem mount. UEFI boot is packaged in the hybrid ISO but is not yet exercised by CI.
