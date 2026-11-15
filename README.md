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

**Architecture and foundation scaffold phase.** The architecture and subsystem contracts are draft specifications, and an initial `no_std` Cargo workspace exists. Limine is selected as the initial bootloader/protocol and normalized boot-information types are scaffolded. No bootable kernel image, architecture entry assembly, implemented PMM/VMM, DKM loader, filesystem, or stable kernel ABI is claimed.

## Documentation

- [Architecture specification](ARCHITECTURE.md)
- [Kernel design](docs/KERNEL_DESIGN.md)
- [DKM specification](docs/DKM_SPEC.md)
- [VFS and filesystem specification](docs/FILESYSTEM_SPEC.md)
- [Memory management specification](docs/MEMORY_MANAGEMENT_SPEC.md)
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

The repository contains a minimal Cargo workspace and an architecture-neutral `wicofe-kernel` library crate using `#![no_std]`. It is still a foundation scaffold; it contains normalized boot-information types and architecture module boundaries, but no Limine protocol adapter, assembly entry point, allocator implementation, panic handler, or linker script.

From the repository root, run:

```sh
cargo check --workspace
```

## Contributing

Contribution guidelines and subsystem-specific requirements will be added as the project moves toward implementation. Do not submit copied or license-incompatible code. All third-party code and dependencies must be reviewed for license, provenance, and compatibility before integration.
