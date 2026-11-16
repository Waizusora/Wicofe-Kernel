# WICOFE

WICOFE is an independent Unix-like operating system project. Its kernel is designed as a configurable monolithic kernel, written primarily in Rust `no_std`, with Assembly reserved for architecture-specific low-level operations.

## Project goals

- Configurable monolithic kernel with built-in, loadable-module, and disabled components.
- Initial CPU targets: x86-64 and AArch64.
- Dynamic Kernel Module (DKM) framework with a documented, versioned ABI.
- Virtual Filesystem (VFS), native ONWFS filesystem, and built-in FAT32 support.
- Unix-like interfaces and a long-term POSIX compatibility objective.
- Original project code licensed under MPL-2.0.

## Current status

**Kernel boot foundation milestone.** The architecture and subsystem contracts are under active development, and the Rust kernel workspace is `no_std`. Boot foundation code includes normalized boot-information types, typed physical/virtual addresses, memory-map validation, allocation-free Limine response adapters, fixed-buffer boot-info normalization, explicit reservations, early usable-memory planning, and HHDM-backed PMM initialization. The x86-64 entry reports boot milestones over COM1 and performs a runtime PMM frame allocate/free smoke test. The PMM remains single-core and is not exposed for general kernel allocations.

The x86-64 freestanding `no_std`/`no_main` binary links as a higher-half ELF executable with RX, R, and RW load segments. CI validates the ELF layout, packages a BIOS Limine ISO, boots it under QEMU, and requires the PMM allocation/free smoke-test marker. AArch64 currently has target compilation coverage only; final ELF and runtime boot validation remain pending. VMM, exception handling, SMP, heap, DKM loading, and filesystems remain under development.

## Documentation

- [Full operating system blueprint](docs/FULL_OS_BLUEPRINT.md)
- [Architecture specification](ARCHITECTURE.md)
- [Kernel design](docs/KERNEL_DESIGN.md)
- [DKM specification](docs/DKM_SPEC.md)
- [Userspace and DKM ABI boundaries](docs/ABI_BOUNDARIES.md)
- [VFS and filesystem specification](docs/FILESYSTEM_SPEC.md)
- [Memory management specification](docs/MEMORY_MANAGEMENT_SPEC.md)
- Typed address primitives: `kernel/src/memory.rs`
- [Boot foundation specification](docs/BOOT_SPEC.md)
- [Licensing policy](docs/LICENSING.md)
- [x86-64 kernel image build](docs/X86_64_KERNEL_IMAGE.md)

The DKM object format/relocation policy and the ONWFS on-disk format remain open design decisions.

## Initial targets and decisions

| Area | Decision |
|---|---|
| Kernel architecture | Configurable monolithic |
| Primary language | Rust `no_std` |
| Low-level language | Assembly |
| CPU targets | x86-64, AArch64 |
| Native filesystem | ONWFS (Oh Nice WICOFE Filesystem) |
| Memory management | Custom Hybrid: bitmap bootstrap/metadata, buddy PMM, VMM, slab kernel heap |
| Bootloader/protocol | Limine / Limine Boot Protocol |
| Additional built-in filesystem | FAT32 |
| Optional filesystems and drivers | DKM, where boot dependencies permit |
| License for original code | MPL-2.0 |

## Initial workspace

The repository contains a minimal Cargo workspace and an architecture-neutral `wicofe-kernel` library crate using `#![no_std]`. The current boot work includes normalized boot-information types, typed address primitives, memory-map validation, Limine response normalization, request registration, and architecture entry trampolines. The freestanding x86-64 ELF image can be built with:

```sh
cargo build -p wicofe-kernel --features kernel-image --bin wicofe-kernel --target x86_64-unknown-none
```

For library development, run from the repository root:

```sh
cargo check --workspace
cargo test --workspace
```

## Contributing

Contribution guidelines and subsystem-specific requirements will be added as the project moves toward implementation. Do not submit copied or license-incompatible code. All third-party code and dependencies must be reviewed for license, provenance, and compatibility before integration.
