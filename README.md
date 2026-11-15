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

**Architecture and foundation scaffold phase.** The architecture and subsystem contracts are draft specifications, and an initial `no_std` Cargo workspace exists. Boot foundation code currently includes normalized boot-information types, typed physical/virtual addresses, memory-map validation, and allocation-free Limine response adapters for the memory map, HHDM offset, executable physical/virtual base addresses, and boot-module physical ranges. The kernel is not yet bootable: Limine request registration, architecture entry assembly, linker-script integration into the final Cargo link, runtime symbol reading, module metadata copying, PMM/VMM, DKM loader, and filesystems are not implemented. Initial x86-64 and AArch64 linker layouts now define `__kernel_start` and `__kernel_end`; a tested constructor derives image length once entry code supplies those bounds.

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

The repository contains a minimal Cargo workspace and an architecture-neutral `wicofe-kernel` library crate using `#![no_std]`. The current boot work includes normalized boot-information types, typed address primitives, memory-map validation, and Limine response normalization for the memory map, HHDM offset, executable address bases, and boot-module physical ranges. It does not yet include Limine request-section registration, an executable entry point, panic handling, linker-script integration, or allocator implementation. Architecture linker-script templates are available under `linker/`.

From the repository root, run:

```sh
cargo check --workspace
cargo test --workspace
```

## Contributing

Contribution guidelines and subsystem-specific requirements will be added as the project moves toward implementation. Do not submit copied or license-incompatible code. All third-party code and dependencies must be reviewed for license, provenance, and compatibility before integration.
