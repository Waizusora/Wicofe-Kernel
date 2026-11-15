# WICOFE Kernel Design

- **Document:** KERNEL_DESIGN.md
- **Version:** 1.0
- **Status:** Draft subsystem contract baseline

## 1. Purpose

This document defines initial subsystem responsibilities, dependency expectations, and cross-subsystem rules. It does not claim that any subsystem is implemented.

## 2. Subsystem map

| Subsystem | Primary responsibility | Initial availability |
|---|---|---|
| Core | Lifecycle, configuration, initialization coordination | Built-in |
| HAL | CPU/platform primitives | Built-in |
| Memory | Physical/virtual memory and allocation | Built-in |
| Interrupt | Interrupt and exception dispatch | Built-in |
| Scheduler | Thread scheduling and context switching coordination | Built-in |
| Process | Process/thread lifecycle and address spaces | Built-in |
| Syscall | User/kernel entry and dispatch | Built-in |
| IPC | Inter-process communication | Built-in foundation |
| Security | Identity, permissions, access control | Built-in foundation |
| VFS | Filesystem-independent interfaces | Built-in |
| DKM | Module lifecycle and loader | Built-in when DKM is enabled |
| Drivers | Hardware-specific support | Per-platform configurable |
| Filesystems | ONWFS, FAT32, and future implementations | ONWFS/FAT32 planned built-in |

A build configuration must reject disabling a subsystem required by another selected component or by the boot path.

## 3. Core

Core coordinates early initialization and kernel-wide services.

Responsibilities:
- Kernel entry handoff.
- Initialization phase orchestration.
- Build/runtime configuration exposure.
- Panic and diagnostic interfaces.
- Logging interface.
- Subsystem registration where needed.

Core must not absorb policy or implementation that belongs to dedicated subsystems.

## 4. HAL and architecture layer

HAL exposes the minimum architecture primitives needed by common kernel code.

Responsibilities:
- CPU and platform initialization.
- CPU feature discovery.
- Timer abstraction.
- Interrupt-controller interface.
- Architecture synchronization primitives.
- Context-switch entry points.
- Page-table and TLB operations.

Architecture-specific code belongs under isolated architecture modules. Common policy remains outside architecture-specific implementations.

## 5. Memory manager

Initial components:
- Early bitmap-based frame tracking and bounded boot allocator.
- Physical Memory Manager (PMM) using a WICOFE-managed buddy allocator, with bitmap and per-frame metadata.
- Virtual Memory Manager (VMM) with architecture-specific page-table backends.
- Slab/size-class kernel heap backed by page allocations.
- Address-space management, mappings, and protection services.

The selected Custom Hybrid Memory Manager architecture and its requirements are specified in [docs/MEMORY_MANAGEMENT_SPEC.md](MEMORY_MANAGEMENT_SPEC.md).

Requirements:
- Allocation ownership and lifetime must be explicit.
- Allocation failure must have defined behavior.
- Mappings must enforce access permissions.
- Address spaces must be isolated.
- SMP-sensitive operations must be synchronized.
- Physical address assumptions must be explicit and validated.

PMM and the minimum VMM functionality precede most other dynamic kernel allocations. The early allocator must hand off ownership cleanly to the main PMM; no physical frame may be simultaneously owned by two allocators.

## 6. Interrupt manager

Responsibilities:
- Exception and interrupt registration.
- Dispatch and acknowledgement coordination.
- Interrupt masking/unmasking.
- Interrupt-context tracking.
- Per-architecture entry/return integration.

Handlers must follow documented interrupt-context restrictions. Blocking operations and potentially unbounded work must not run in hard interrupt context unless explicitly justified by the architecture and implementation contract.

## 7. Scheduler

Responsibilities:
- Runnable-thread queues.
- Thread state transitions.
- CPU selection and affinity foundations.
- Scheduling policy.
- Preemption coordination.
- Context-switch coordination.
- SMP synchronization.

Initial thread states: New, Ready, Running, Blocked, and Terminated.

The scheduling algorithm is intentionally TBD. Evaluation criteria include latency, fairness, throughput, real-time requirements if adopted, and multicore scalability.

## 8. Process and thread manager

Responsibilities:
- Process and thread creation/termination.
- Process and thread identity.
- Address-space association.
- Parent/child relationships.
- Resource ownership and cleanup.
- Process state transitions.

The design must distinguish process-level resources from thread-level execution state.

## 9. System call subsystem

Responsibilities:
- Architecture-specific syscall entry.
- Common syscall dispatch.
- Argument validation.
- Permission checks.
- Error and return-value conventions.
- ABI version documentation.

Architecture-specific entry code must be separated from syscall implementation and policy. The syscall ABI is a user-visible compatibility contract and requires explicit versioning discipline.

## 10. IPC subsystem

Target mechanisms include:
- Pipes.
- Signals.
- Shared memory.
- Message-based communication.
- Synchronization primitives.

The IPC design must specify object lifetime, permissions, blocking semantics, cancellation behavior, and resource limits.

## 11. Security subsystem

Responsibilities:
- UID/GID identity model.
- Permission evaluation.
- Privilege separation.
- Kernel object access control.
- Capability-oriented internal interfaces.
- Security policy hooks.

Subsystems must call a shared access-control interface rather than implementing inconsistent permission checks independently.

## 12. VFS subsystem

Initial abstractions:
- Filesystem instance and operations.
- Mount and mount namespace policy.
- Inode/vnode-like object.
- Directory/path lookup object.
- Open file object.
- File descriptor table integration.
- Metadata and operation capability reporting.

VFS must define ownership, reference counting, locking, path traversal, mount lifetime, and error semantics before filesystem implementation begins.

A filesystem may report an operation as unsupported. VFS must not fabricate symlink, hard-link, permission, or durability semantics that the underlying filesystem cannot provide.

## 13. DKM subsystem

Responsibilities:
- Module format recognition.
- Metadata and ABI validation.
- Architecture validation.
- Relocation and symbol resolution.
- Dependency checks.
- Initialization and shutdown callbacks.
- Reference tracking.
- Safe-unload decisions.

A module must not be unloaded while executing or while reachable through registered callbacks, interrupts, device operations, or other live references. If safe quiescence cannot be established, unload is rejected.

## 14. Driver framework

Responsibilities:
- Device identity and discovery integration.
- Driver matching and binding.
- Device lifecycle.
- Resource ownership.
- Interrupt and DMA coordination.
- Power-management hooks when designed.

Boot-critical drivers must be available before the boot phase that requires them. Optional drivers may be DKM.

## 15. Initialization dependency outline

1. Limine handoff validation through the architecture-specific entry adapter.
2. Normalization of boot information and reservation of kernel/module ranges.
3. Early core and minimal HAL.
4. PMM.
5. Initial VMM and kernel mappings.
6. Interrupt controller and timer.
7. Kernel heap and synchronization foundations.
8. Scheduler and idle threads.
9. Process/thread services.
10. IPC and security services.
11. VFS core.
12. DKM manager.
13. Device discovery and required drivers.
14. Root filesystem mount.

The boot handoff contract is specified in [docs/BOOT_SPEC.md](BOOT_SPEC.md). The final sequence must be validated against each supported platform. Circular dependencies must be resolved explicitly, not hidden by initialization order assumptions.

## 16. Cross-subsystem contract rules

1. Public internal interfaces must be documented.
2. Private implementation details must not be accessed across subsystem boundaries.
3. Resource ownership and lifetime must be explicit.
4. Lock ordering and lock context must be documented.
5. Interrupt context and normal execution context must be distinguished.
6. Cross-module ABI must not depend on Rust's unstable ABI or trait-object layout.
7. Errors must use consistent, documented semantics.
8. Unsafe code must document invariants and safety rationale.
9. Interfaces must be testable independently where practical.
10. Changes to shared contracts require review of all dependent subsystems.

## 17. Rust crate organization

Initial logical layout:

```text
kernel/
├── core/
├── hal/
├── memory/
├── interrupt/
├── scheduler/
├── process/
├── syscall/
├── ipc/
├── security/
├── vfs/
└── dkm/

arch/
├── x86_64/
└── aarch64/

drivers/
filesystems/
```

This is a logical subsystem map, not yet a finalized Cargo workspace. Crate boundaries and dependency direction will be frozen during the foundation stage.

## 18. Subsystem completion criteria

A subsystem is not complete merely because it compiles. Completion requires:
- Reviewed interface and invariants.
- Dependency and safety review.
- Unit tests where applicable.
- Integration tests.
- Architecture-specific tests where relevant.
- Failure-path validation.
- Documentation updates.

**End of kernel design baseline.**
