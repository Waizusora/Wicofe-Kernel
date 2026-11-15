# WICOFE

WICOFE is an independent Unix-like operating system project. Its kernel is designed as a configurable monolithic kernel, written primarily in Rust `no_std`, with Assembly reserved for architecture-specific low-level operations.

## Project goals

- Configurable monolithic kernel with built-in, loadable-module, and disabled components.
- Initial CPU targets: x86-64 and AArch64.
- Dynamic Kernel Module (DKM) framework with a documented, versioned ABI.
- Virtual Filesystem (VFS), native ONWFS filesystem, and built-in FAT32 support.
- Unix-like interfaces and a long-term POSIX compatibility objective.
- Original project code licensed under MPL-2.0.

WICOFE is an independent implementation. It is not a Linux or BSD derivative.

## Current status

**Architecture and design phase.** No bootable kernel or stable kernel ABI is claimed at this stage. Specifications are being established before implementation.

## Documentation

- [Architecture specification](ARCHITECTURE.md)
- [Kernel design](docs/KERNEL_DESIGN.md)
- [Licensing policy](docs/LICENSING.md)

Further specifications will cover DKM and filesystem formats.

## Initial targets

| Area | Decision |
|---|---|
| Kernel architecture | Configurable monolithic |
| Primary language | Rust `no_std` |
| Low-level language | Assembly |
| CPU targets | x86-64, AArch64 |
| Native filesystem | ONWFS (Oh Nice WICOFE Filesystem) |
| Additional built-in filesystem | FAT32 |
| Optional filesystems and drivers | DKM, where boot dependencies permit |
| License for original code | MPL-2.0 |

## Contributing

Contribution guidelines and subsystem-specific requirements will be added as the project moves toward implementation. Do not submit copied or license-incompatible code. All third-party code and dependencies must be reviewed for license obligations before integration.

## License

Original source files in this repository are made available under the Mozilla Public License 2.0. See [LICENSE](LICENSE) and [docs/LICENSING.md](docs/LICENSING.md). Third-party components, when approved, remain under their respective licenses.
