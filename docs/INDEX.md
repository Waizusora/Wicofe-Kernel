# Kernel Documentation Index

This directory is exclusively for Wicofe kernel architecture, implementation contracts, and kernel validation. It does not document the whole OS or userspace product behavior.

## Canonical kernel documents

- [Kernel design](KERNEL_DESIGN.md)
- [DKM specification](DKM_SPEC.md)
- [ABI boundaries](ABI_BOUNDARIES.md)
- [VFS and filesystem integration](FILESYSTEM_SPEC.md)
- [Memory management](MEMORY_MANAGEMENT_SPEC.md)
- [Boot protocol and handoff](BOOT_SPEC.md)
- [x86-64 kernel image](X86_64_KERNEL_IMAGE.md)
- [Full OS blueprint reference](FULL_OS_BLUEPRINT.md) — architecture context only; kernel-owned contracts remain the documents above.

## Kernel scope

Kernel-owned topics include CPU architecture support, boot handoff, interrupts and exceptions, physical/virtual memory, scheduling and process primitives, syscall dispatch implementation, VFS internals, block I/O, driver lifecycle, and the privileged DKM ABI.

The userspace syscall contract is owned by [Wicofe-ABI](https://github.com/Waizusora/Wicofe-ABI). The ONWFS disk format is owned by [ONWFS](https://github.com/Waizusora/ONWFS). Cross-repository contracts are coordinated by [Wicofe-Specs](https://github.com/Waizusora/Wicofe-Specs).

## Status rules

Every document must distinguish implemented, compiled, emulator-tested, hardware-tested, proposed, and deferred work. A successful cross-compilation is not runtime boot validation. Do not describe an unfinished subsystem as available.

## Related documentation

- [Central Wicofe documentation index](https://github.com/Waizusora/Wicofe-Docs/blob/main/INDEX.md)
- [Repository documentation map](https://github.com/Waizusora/Wicofe-Docs/blob/main/REPOSITORY_DOCUMENTATION_MAP.md)
