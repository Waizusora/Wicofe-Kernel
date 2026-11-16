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

**Architecture and foundation scaffold phase.** The architecture and subsystem contracts are draft specifications, and an initial `no_std` Cargo workspace exists. Boot foundation code includes normalized boot-information types, typed physical/virtual addresses, memory-map validation, allocation-free Limine response adapters plus an end-to-end fixed-buffer normalizer for validated boot information, and static Limine request registration for memory map, HHDM, executable address, modules, framebuffer, and a 64 KiB stack. The request records target protocol base revision 6 and are retained in writable linker data because Limine patches response pointers. Minimal x86-64 and AArch64 Limine entry trampolines now call a shared Rust bootstrap. The bootstrap passes Limine responses through the allocation-free consumer, uses linker image bounds and fixed-capacity early buffers (256 memory-map entries and 64 modules), and validates a borrowed `BootInfo`, including kernel/module physical-range containment and non-overlapping reservations, before fail-stopping. The x86-64 Cargo binary now has a freestanding `no_std`/`no_main` entry image, a higher-half linker-script configuration, and a bounded COM1 panic diagnostic. CI links the x86-64 ELF and inspects its ELF/program headers. This is a build artifact milestone only: a Limine boot image and QEMU runtime validation are still pending. AArch64 remains target-checked but does not yet have final ELF integration. PMM/VMM, DKM loader, and filesystems remain under development. Initial x86-64 and AArch64 linker layouts define `__kernel_start` and `__kernel_end`; both use the Limine higher-half minimum virtual address.

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

The repository contains a minimal Cargo workspace and an architecture-neutral `wicofe-kernel` library crate using `#![no_std]`. The current boot work includes normalized boot-information types, typed address primitives, memory-map validation, Limine response normalization, request registration, and minimal architecture entry trampolines. The Rust bootstrap consumes and validates Limine responses into fixed-capacity early storage before fail-stopping. It is not yet a final executable: panic handling, final linker-script integration, early diagnostics, and allocator implementation are still pending. Architecture linker-script templates are available under `linker/`.

From the repository root, run:

```sh
cargo check --workspace
cargo test --workspace
```

## Contributing

Contribution guidelines and subsystem-specific requirements will be added as the project moves toward implementation. Do not submit copied or license-incompatible code. All third-party code and dependencies must be reviewed for license, provenance, and compatibility before integration.
