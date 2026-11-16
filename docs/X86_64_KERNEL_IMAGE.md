# x86-64 Kernel Image and Boot Validation

This document describes the current x86-64 freestanding ELF build and the BIOS/Limine ISO runtime path. The ELF executable and bootable ISO are distinct artifacts: the former is the kernel payload; the latter adds the pinned Limine bootloader and its required files.

## Build the kernel ELF

From the repository root, with the pinned Rust toolchain and target installed:

```sh
rustup target add x86_64-unknown-none
cargo build -p wicofe-kernel --features kernel-image --bin wicofe-kernel --target x86_64-unknown-none
```

The `kernel-image` feature enables the freestanding binary while keeping it out of host `cargo test --workspace`. Target configuration applies the x86-64 kernel code model, static relocation, disabled red zone, panic-abort behavior, and `linker/x86_64.ld`. The output is `target/x86_64-unknown-none/debug/wicofe-kernel`.

## Validate the ELF layout

```sh
llvm-readelf-22 -h target/x86_64-unknown-none/debug/wicofe-kernel
llvm-readelf-22 -l target/x86_64-unknown-none/debug/wicofe-kernel
python3 scripts/verify_kernel_elf.py target/x86_64-unknown-none/debug/wicofe-kernel
```

The verifier checks ELF64 little-endian format, x86-64 `ET_EXEC`, the higher-half entry point, exactly three loadable segments, 4 KiB alignment, VMA/LMA offset, ordered physical load addresses, valid file/memory sizes, and non-writable-executable segment permissions.

## Build and boot the BIOS/Limine ISO

The current CI path packages the ELF with the pinned Limine v12.9.6 BIOS boot files:

```sh
scripts/build_limine_iso.sh target/wicofe-x86_64.iso
timeout 20s qemu-system-x86_64 \
  -machine q35 -m 512M -cdrom target/wicofe-x86_64.iso -boot d \
  -display none -serial file:target/qemu-serial.log \
  -no-reboot -no-shutdown
cat target/qemu-serial.log
```

The successful QEMU smoke test requires these serial markers:

- `WICOFE: BOOTSTRAP_ENTERED`
- `WICOFE: BOOTINFO_VALID`
- `WICOFE: PMM_READY`
- `WICOFE: PMM_ALLOC_FREE_OK`

CI also rejects explicit bootstrap or PMM smoke-test failure markers and verifies that the ISO contains the Limine license notices. The ISO SHA-256 is recorded as a workflow artifact.

## Current validation boundary

The latest inspected Kernel Actions run on 2026-11-16 (run `43882914312`, commit `2f529b0d6513d03ec99ddc6a5a3555692e989923`) completed successfully, including formatting, workspace unit tests, x86-64 target check, ELF build/layout verification, ISO packaging, license checks, and QEMU boot assertions.

This evidence proves only the tested BIOS/QEMU configuration and the early boot/PMM allocate-free path. It does not prove VMM runtime operation, exception or interrupt handling, SMP, general heap allocation, userspace execution, filesystem mounting, UEFI boot, or hardware compatibility.

AArch64 currently has target compilation coverage only. Final ELF validation and runtime boot under an AArch64 emulator or hardware remain separate milestones.

## Next kernel milestones

1. Validate VMM mapping, translation, unmapping, permission, and TLB contracts.
2. Add architecture exception/interrupt entry and bounded diagnostics.
3. Establish timer and synchronization foundations.
4. Implement heap, threads, scheduler, process/address-space, and syscall support in dependency order.
5. Integrate native Osynit PID 1 and a CLI only after the required process and userspace contracts exist.

See [Kernel documentation index](INDEX.md), [Limine QEMU boot procedure](LIMINE_QEMU_BOOT.md), and the [bootable vertical slice roadmap](https://github.com/Waizusora/Wicofe-Specs/blob/main/architecture/BOOTABLE_VERTICAL_SLICE_ROADMAP_0.1.md).
