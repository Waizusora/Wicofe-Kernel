# Wicofe Kernel

Wicofe-Kernel is the kernel implementation repository for Wicofe OS. It develops a configurable **monolithic Rust `no_std` kernel** with architecture-specific assembly where required.

## Kernel responsibilities

- x86-64 and AArch64 architecture bring-up.
- Limine boot protocol handling and normalized boot information.
- CPU, exception, interrupt, and early platform support.
- Physical and virtual memory management.
- Scheduler, threads, processes, and kernel synchronization primitives.
- Syscall dispatch implementation.
- Kernel VFS, block I/O, device model, and driver lifecycle.
- Dynamic Kernel Modules (DKM) loader and privileged module ABI.
- Built-in filesystem integration and boot-critical storage paths.

Optional DKM components extend the monolithic kernel; they do not change the kernel architecture into a microkernel.

## Current implementation status

**Status: kernel boot foundation milestone.** The repository contains a Rust `no_std` workspace and architecture-neutral kernel library. Current boot foundation work includes normalized boot-information types, typed physical/virtual addresses, memory-map validation, allocation-free Limine response adapters, fixed-buffer boot-info normalization, explicit memory reservations, early usable-memory planning, and HHDM-backed PMM initialization.

The x86-64 entry reports boot milestones over COM1 and performs runtime PMM frame allocate/free validation. The legacy 8259 PIC is remapped and kept masked by default; a build-only QEMU smoke test validates the returning IRQ0 path, PIT channel 0 delivery, handler dispatch and `iretq`. A PIT-backed monotonic tick counter exposes ticks and tick-derived uptime but does not enable interrupts globally. The PMM remains single-core and is not yet exposed for general kernel allocations.

The freestanding x86-64 `no_std`/`no_main` binary links as a higher-half ELF executable with RX, R, and RW load segments. CI validation includes ELF layout checks, BIOS Limine ISO packaging, QEMU boot, and a required PMM smoke-test marker. AArch64 currently has target compilation coverage; final ELF and runtime boot validation remain pending.

VMM, exception handling, SMP, heap allocation, DKM loading, and filesystem implementations remain under development. These status statements must be updated only when corresponding code and validation evidence change.

## Architecture decisions

| Area | Kernel decision |
|---|---|
| Kernel architecture | Configurable monolithic |
| Primary language | Rust `no_std` |
| Low-level language | Architecture-specific assembly |
| Initial CPU targets | x86-64, AArch64 |
| Boot protocol | Limine |
| Module model | Versioned Dynamic Kernel Modules |
| Filesystem architecture | Kernel-owned VFS; ONWFS and FAT32 integration planned |
| Kernel memory model | Bootstrap allocator, PMM, VMM, kernel heap (staged implementation) |
| Original code license | MPL-2.0 |

## Init-system integration

[Frenit (Free Init)](https://github.com/Waizusora/Frenit) is the canonical Wicofe init-system and service-management project. The kernel provides the process, syscall, VFS, IPC, signal, and power-management mechanisms Frenit will consume; it does not own init policy. Native Frenit PID 1 handoff is not implemented or verified yet. See [Frenit integration boundary](docs/FRENIT_INTEGRATION.md) and the [cross-component contract](https://github.com/Waizusora/Wicofe-Specs/blob/main/boot/FRENIT_INTEGRATION_CONTRACT_0.1.md).

## Kernel documentation

Start at [docs/INDEX.md](docs/INDEX.md). Canonical kernel documents include:

- [Kernel design](docs/KERNEL_DESIGN.md)
- [Boot specification](docs/BOOT_SPEC.md)
- [Memory management specification](docs/MEMORY_MANAGEMENT_SPEC.md)
- [DKM specification](docs/DKM_SPEC.md)
- [VFS and filesystem specification](docs/FILESYSTEM_SPEC.md)
- [Kernel ABI boundaries](docs/ABI_BOUNDARIES.md)
- [x86-64 kernel image build](docs/X86_64_KERNEL_IMAGE.md)

The userspace syscall ABI is owned by [Wicofe-ABI](https://github.com/Waizusora/Wicofe-ABI). ONWFS disk layout is owned by [ONWFS](https://github.com/Waizusora/ONWFS). Cross-repository contracts are coordinated in [Wicofe-Specs](https://github.com/Waizusora/Wicofe-Specs).

## Third-party licensing

Original Wicofe-Kernel code uses MPL-2.0. Limine 12.9.6 is pinned by release version and SHA-256; its BSD-2-Clause and bundled third-party notices are staged into the ISO. See [third-party licensing evidence](docs/licensing.md).

## Build and test

Build the x86-64 freestanding kernel image from the repository root:

```sh
cargo build -p wicofe-kernel --features kernel-image --bin wicofe-kernel --target x86_64-unknown-none
```

Run workspace checks:

```sh
cargo check --workspace
cargo test --workspace
```

These commands are local development entry points. Their presence does not imply that every architecture or subsystem passes runtime validation. Consult CI results and the relevant subsystem document for actual evidence.

## Contribution rules

- Keep kernel code `no_std` unless a reviewed architecture decision explicitly changes that boundary.
- Do not expose Rust compiler-private layout or trait-object ABI across DKM or userspace boundaries.
- Keep syscall ABI definitions in Wicofe-ABI; do not independently assign syscall numbers here.
- Validate all bootloader, device, and disk-provided input as untrusted.
- Document architecture-specific assumptions and test evidence.
- Review dependency licenses and provenance; original kernel code targets MPL-2.0.
