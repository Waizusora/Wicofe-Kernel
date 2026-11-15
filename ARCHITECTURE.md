# WICOFE Kernel Architecture Specification

- **Project:** WICOFE OS
- **Component:** WICOFE Kernel
- **Version:** 1.0
- **Status:** Architecture baseline (initial; open decisions remain)
- **Date:** 2026-11-15

## 1. Purpose and scope

WICOFE is an independent Unix-like operating system project. This document establishes the initial architectural baseline for its kernel. It is a design specification, not evidence that the described features have been implemented.

The initial effort focuses on the kernel and its contracts. A complete operating system, userland, desktop environment, package manager, and POSIX-conforming system are outside the initial kernel foundation milestone.

### Goals

- Configurable monolithic kernel.
- Rust `no_std` as the primary implementation language.
- Assembly for architecture-specific entry, context switching, and other low-level operations.
- Initial support for x86-64 and AArch64.
- Dynamic Kernel Modules (DKM).
- Virtual Filesystem (VFS).
- Native ONWFS filesystem and built-in FAT32 support.
- Unix-like kernel interfaces and a path toward POSIX compatibility.
- MPL-2.0 for original project code, with a non-GPL dependency policy.

## 2. Kernel architecture

### 2.1 Configurable monolithic design

WICOFE is a monolithic kernel. Kernel subsystems execute in kernel space and may interact through defined in-kernel interfaces.

Each configurable component has one build mode:

| Mode | Meaning |
|---|---|
| `built-in` | Linked into the kernel image |
| `module` | Supplied as a loadable DKM |
| `disabled` | Excluded from the build |

The build configuration must reject invalid combinations. A component required to reach and mount the root filesystem must be built in or supplied by a boot-time module mechanism available before it is needed.

DKM is a loadable component mechanism; it does not make WICOFE a microkernel. A loaded kernel module has kernel-space privilege and must be treated as trusted, security-sensitive code.

### 2.2 Core and optional components

The initial built-in baseline includes architecture entry and HAL essentials, memory management, interrupt/exception foundations, scheduler foundations, process/thread foundations, syscall entry, VFS core, security primitives, and DKM management when dynamic loading is enabled.

Drivers and optional subsystems may be built-in, modular, or disabled according to target configuration and boot dependencies. Exact defaults remain subject to the target platform profile.

## 3. Language and implementation model

### 3.1 Rust

Kernel code uses Rust `no_std`. The design should use Rust ownership and type-safety features where practical while explicitly documenting unavoidable unsafe boundaries.

The kernel must not assume the availability of the Rust standard library or a hosted operating-system runtime.

### 3.2 Assembly and FFI

Assembly is limited to operations requiring direct architecture-level control, including boot entry, CPU register operations, interrupt entry/return, context-switch primitives, and page-table transitions.

FFI is permitted where a stable low-level interface or interoperability requires it. It is not the default internal programming model.

Rust trait object layout and compiler-specific Rust ABI must not be used as a stable cross-module ABI.

## 4. Supported architectures

Initial targets:

- x86-64
- AArch64

Architecture-specific code must be isolated behind documented interfaces. The project does not target x86 32-bit or ARM 32-bit in the initial scope.

Minimum CPU feature requirements, platform profiles, and boot handoff protocols remain open decisions.

## 5. Hardware Abstraction Layer

The HAL separates common kernel logic from CPU and platform details. Its responsibilities include CPU initialization, timer access, interrupt-controller integration, architecture-specific synchronization, context-switch primitives, CPU feature discovery, and memory-management hardware operations.

The HAL must not become a generic dumping ground for unrelated subsystem policy.

## 6. Kernel subsystems

The initial subsystem boundaries are:

- **Core:** lifecycle, initialization ordering, configuration, panic and logging interfaces.
- **HAL:** architecture and platform primitives.
- **Memory manager:** a Custom Hybrid Memory Manager comprising an early bitmap-based allocator, a buddy-based PMM with bitmap/frame metadata, an architecture-backed VMM, and a slab/size-class kernel heap. See [docs/MEMORY_MANAGEMENT_SPEC.md](docs/MEMORY_MANAGEMENT_SPEC.md).
- **Interrupt manager:** interrupt registration and dispatch, exception handling, and interrupt-context rules.
- **Scheduler:** runnable threads, scheduling policy, CPU selection, and context switching.
- **Process/thread manager:** lifecycle, address-space association, resource ownership, and process relationships.
- **Syscall subsystem:** controlled user-to-kernel entry, argument validation, dispatch, and error conventions.
- **IPC:** process communication and synchronization interfaces.
- **Security:** identity, permissions, privilege boundaries, and object access checks.
- **VFS:** common filesystem and file-object abstractions.
- **DKM manager:** module validation, loading, dependency handling, lifecycle, and unloading.
- **Driver framework:** device discovery, resource ownership, driver lifecycle, and subsystem integration.

Subsystem contracts are elaborated in [docs/KERNEL_DESIGN.md](docs/KERNEL_DESIGN.md).

## 7. Virtual Filesystem

VFS provides a filesystem-independent interface for path lookup, mount management, file and directory operations, metadata, and file-object handling.

The initial conceptual objects include filesystem instances, mounts, inode/vnode-like objects, directory/path components, open file objects, and file descriptors. Exact object ownership and locking rules must be specified before implementation.

Filesystem capabilities must be represented honestly. A filesystem that does not implement symlinks, hard links, or another operation must return a defined unsupported-operation result rather than emulate unsupported semantics incorrectly.

## 8. Filesystem policy

### 8.1 ONWFS

**ONWFS — Oh Nice WICOFE Filesystem** is the selected name for WICOFE's native filesystem.

The intended design targets an inode-based hierarchy, directories, symbolic and hard links, UID/GID, POSIX-style permissions, timestamps, extended attributes, sparse files, metadata checksums, journaling, crash recovery, and storage-aware allocation.

These are design goals, not implemented or finalized format guarantees. The on-disk format, journal protocol, recovery rules, checksums, endian policy, feature flags, and versioning must be specified independently before implementation.

### 8.2 FAT32

FAT32 is planned as an additional built-in filesystem for interoperability and removable media. FAT32 does not natively provide all POSIX filesystem semantics; VFS must expose those limitations accurately.

### 8.3 Other filesystems

Additional local or network filesystems may be provided as DKM when their dependencies and boot requirements permit. NFS means Network File System and is not a local on-disk filesystem format.

## 9. Dynamic Kernel Modules

DKM permits selected kernel components to be loaded after boot while preserving the monolithic architecture.

The module ABI must be explicitly versioned and C-compatible at the boundary. Candidate mechanisms include `repr(C)` structures, function tables, explicit metadata, version negotiation, relocation support, dependency declarations, and reference-counted lifecycle management.

The loader must validate module format, architecture, ABI compatibility, dependencies, and integrity before initialization. Signature verification and trust-root policy must be specified before enabling untrusted module sources.

Safe unloading requires proof that no active references, callbacks, interrupts, or executing code can reach the module. If that cannot be established, unloading must be refused.

## 10. Driver architecture

Drivers may be built-in or supplied as DKM. Storage and bus drivers required to locate the root filesystem must be available before root mounting, either built in or through a defined early-boot module path.

Driver categories may include storage, USB, networking, audio, display, input, Bluetooth, and Wi-Fi. The initial supported hardware set remains open.

## 11. Boot sequence

The conceptual boot phases are:

1. Firmware and bootloader preparation.
2. Boot protocol handoff to architecture entry.
3. Early kernel initialization.
4. HAL and CPU initialization.
5. Physical and virtual memory initialization.
6. Interrupt and timer initialization.
7. Kernel heap and scheduler initialization.
8. Process and core-service initialization.
9. Device discovery and required driver initialization.
10. VFS initialization and root filesystem mounting.
11. Handoff to the next operating-system stage.

This sequence is a dependency outline, not a finalized boot protocol. Bootloader choice, handoff data structures, relocation model, and early module delivery remain TBD.

## 12. Unix-like and POSIX direction

The kernel is intended to provide foundations for processes, threads, file descriptors, filesystem operations, permissions, signals, pipes, IPC, and system calls.

POSIX conformance applies to a broader system interface, including user-space libraries and utilities. A kernel-only implementation must not be described as a POSIX-conforming operating system.

## 13. Licensing

Original WICOFE source code is licensed under MPL-2.0.

The project policy is not to integrate GPL-, LGPL-, or AGPL-licensed code or dependencies. Permissively licensed dependencies may be considered after review of license text, notices, patent terms, provenance, and compatibility with the project's distribution model.

Third-party code retains its original license. No third-party code may be relicensed merely by inclusion in this repository. CI should eventually enforce dependency and license auditing.

## 14. Repository and development policy

Subsystems must be designed and implemented in stages. Each subsystem requires a documented interface, dependency analysis, tests appropriate to its behavior, and integration validation.

A successful compilation alone does not establish correctness or completion. Architecture-specific builds, boot tests, memory tests, filesystem consistency tests, DKM lifecycle tests, and ABI checks are required as the project matures.

## 15. Roadmap

1. Architecture baseline and subsystem contracts.
2. Rust workspace, boot foundation, and HAL.
3. Custom Hybrid Memory Manager: early allocator, buddy PMM, VMM, and kernel heap.
4. Interrupts, timers, and CPU management.
5. Scheduler and process/thread lifecycle.
6. Syscall ABI and IPC.
7. VFS interfaces.
8. ONWFS and FAT32.
9. DKM loader and ABI.
10. Driver framework and initial hardware support.
11. Integration, testing, and hardening.
12. User-space foundation and POSIX development.

The exact implementation order may change when dependency analysis demonstrates a better sequence. Changes to architectural decisions must be recorded.

## 16. Open decisions

The following are deliberately not frozen by this baseline:

- Bootloader and boot protocol.
- Minimum CPU features and platform profiles.
- PMM implementation parameters (buddy order limits, metadata encoding, zones, and synchronization).
- Virtual address-space layout.
- Scheduler algorithm and policy model.
- Syscall numbering and calling conventions.
- IPC object model.
- DKM executable format and relocation rules.
- Module-signing trust model.
- ONWFS on-disk format and journal protocol.
- Default configuration per platform.
- Initial hardware support matrix.

**End of architecture baseline.**
