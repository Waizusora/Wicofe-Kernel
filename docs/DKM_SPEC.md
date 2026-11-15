# WICOFE Dynamic Kernel Module (DKM) Specification

- **Document:** DKM_SPEC.md
- **Version:** 0.1
- **Status:** Draft contract specification
- **Scope:** Kernel-space loadable modules for the configurable monolithic WICOFE kernel

## 1. Purpose

This document defines security, compatibility, lifecycle, and integration requirements for Dynamic Kernel Modules (DKM). It is a contract baseline, not a statement that a loader, module ABI, or module format has been implemented.

A DKM executes with kernel privilege. Loading a module is equivalent to admitting privileged code into the kernel trust boundary. DKM is not a userspace driver framework and does not change WICOFE's monolithic architecture.

## 2. Design principles

1. The kernel remains monolithic; modules execute in kernel space.
2. Components are selected as `built-in`, `module`, or `disabled` at configuration/build time.
3. A module may only depend on explicitly exported kernel interfaces.
4. The module boundary must not depend on Rust's unstable ABI, compiler-specific layout, or Rust trait-object representation.
5. Every module and interface is architecture- and ABI-version-aware.
6. Validation must complete before module initialization.
7. Unload is permitted only after the module is proven quiescent and unreachable.
8. A failed load must roll back all resources acquired during that attempt.

## 3. Component configuration

| Mode | Build and runtime behavior |
|---|---|
| `built-in` | Linked into the kernel image; no runtime module load required |
| `module` | Built as a DKM and loaded by the kernel module manager |
| `disabled` | Not built or exposed |

Configuration validation must reject:
- A built-in component depending on a disabled component.
- A module depending on an interface not exported by the selected kernel configuration.
- A boot-critical component unavailable before its first required use.
- Circular or unsatisfied module dependencies.

The exact configuration file format and Kconfig-like interface remain open.

## 4. Module trust and security model

A DKM is fully privileged. ABI validation does not sandbox malicious or defective module code.

Before production module loading is enabled, the design must specify:
- Trusted signing keys and key enrollment.
- Signature algorithm and key-rotation policy.
- Revocation and rollback policy.
- Whether development builds may accept unsigned modules.
- How module provenance and build identity are recorded.

Integrity verification must cover the exact bytes that will be loaded, including metadata relevant to relocation and symbol resolution. A checksum alone detects accidental corruption but does not authenticate the publisher.

A module must not be loaded from an untrusted source merely because its filename, metadata, or ABI version appears valid.

## 5. Module metadata

Each module must provide a bounded, parseable metadata record containing at least:

- Magic and metadata format version.
- Module name and unique identity.
- Module version.
- Target architecture and required CPU features, if any.
- Required kernel DKM ABI version or compatible version range.
- Required exported kernel interfaces.
- Declared module dependencies and version constraints.
- Initialization and shutdown entry points.
- License identifier and provenance information.
- Integrity/signature data or a reference to its authenticated envelope.

Metadata parsing must be length-bounded, overflow-safe, and independent of untrusted pointer dereferences. Names and dependency counts must have explicit maximum lengths/counts.

The binary container and exact metadata encoding are TBD. ELF is a candidate, not a selected requirement.

## 6. ABI boundary

The stable boundary should use fixed-width integer types, explicitly sized fields, opaque handles, and `repr(C)`-compatible structures.

Rules:
- No Rust references, slices, `String`, `Vec`, trait objects, closures, or compiler-owned types cross the ABI.
- Pointer ownership, mutability, alignment, and lifetime must be documented for every pointer field.
- Structures must include a size and/or version field to permit compatible extension.
- Reserved fields must be zero on input and ignored on output unless a later ABI version defines them.
- Function tables must declare their ABI version and supported capabilities.
- Errors use documented numeric status codes; they must not expose Rust enum layout.
- Panics must never unwind across the module boundary.

A C-compatible ABI is a representation contract, not a promise that arbitrary C code is safe to execute in kernel mode.

## 7. Symbol and relocation policy

The loader must resolve only explicitly exported kernel symbols and approved module-to-module exports.

The final design must define:
- Supported object format and machine types.
- Relocation types supported for x86-64 and AArch64.
- Symbol visibility and namespace rules.
- Duplicate symbol handling.
- Weak/optional symbol behavior, if supported.
- Position-independent versus fixed-address module policy.
- W^X memory permissions during and after relocation.
- Instruction-cache synchronization requirements on architectures that need it.

Unknown relocation types, out-of-range relocations, malformed symbol tables, and relocation arithmetic overflow must fail the load.

The exact object format, relocation subset, and module address-space layout remain open decisions.

## 8. Loading transaction

A module load is a transaction. The conceptual sequence is:

1. Read module bytes into kernel-controlled memory.
2. Validate container bounds and metadata.
3. Check architecture, CPU requirements, kernel ABI, and configuration compatibility.
4. Authenticate module integrity and signature according to the active trust policy.
5. Resolve dependencies and reject cycles or incompatible versions.
6. Reserve memory and perform validated relocations.
7. Apply final memory permissions; executable memory must not remain writable.
8. Resolve the module's versioned entry interface.
9. Initialize the module.
10. Publish registrations and mark the module active only after successful initialization.

If any stage fails, reverse completed stages in a defined order, unregister partial publications, release references, and free allocations. A partially initialized module must never be reported as active.

## 9. Dependencies and symbol exports

Dependencies must be explicit and version-constrained. The loader must detect dependency cycles before initialization and produce a useful diagnostic chain.

Kernel exports should be grouped into versioned namespaces or interface sets. Exporting an internal implementation symbol is prohibited unless it is intentionally part of the supported DKM contract.

A module dependency pins the required module against unloading. Optional dependencies, if introduced, must have explicit runtime absence semantics.

## 10. Lifecycle and concurrency

Minimum lifecycle states:

- Discovered
- Validating
- Loading
- Initializing
- Active
- Quiescing
- Unloading
- Failed

Only valid state transitions may be performed. State changes and module reference accounting must be synchronized.

The module manager must track references arising from:
- Other modules.
- Registered callbacks and notifiers.
- Interrupt handlers.
- Device bindings and in-flight I/O.
- Timers, work queues, and deferred execution.
- Threads or code currently executing inside the module.
- Any kernel object containing a module-owned function pointer.

## 11. Safe unload

Unload must be rejected unless all of the following are true:

1. The module is not required by another active module.
2. New work and new references can be prevented.
3. Registered devices, callbacks, interrupts, timers, and notifiers are detached.
4. In-flight operations have completed or been safely cancelled.
5. No CPU can still execute module code or return into it.
6. All module-owned objects and function pointers are unreachable.
7. The shutdown callback completes under a documented contract.

The manager must first transition to Quiescing, prevent new entry points, drain or cancel outstanding work, and then release resources. A timeout is not proof of safety. If quiescence cannot be proven, the module remains loaded.

## 12. Boot-time modules

Storage, bus, and filesystem modules required to locate the root filesystem must be available before the relevant boot phase. The boot protocol must define how early modules are authenticated, described, and passed to the kernel.

A module required to mount the root filesystem must not itself depend on that filesystem for retrieval.

## 13. Failure handling and diagnostics

Every failure path must:
- Return a stable error code.
- Identify the module and failed stage in diagnostics.
- Avoid leaking memory, symbol references, or registrations.
- Leave unrelated active modules unaffected.
- Preserve kernel integrity; fatal invariant violations may require a controlled kernel panic.

Malformed module input must be treated as untrusted data.

## 14. Testing requirements

Before DKM is considered usable, tests must cover:
- Truncated and malformed containers.
- Integer overflow and out-of-bounds metadata.
- Wrong architecture and unsupported CPU features.
- ABI mismatch and missing exports.
- Invalid and overflowing relocations.
- Bad signatures, revoked keys, and tampered bytes.
- Dependency cycles and version conflicts.
- Initialization failure and rollback.
- Concurrent load/unload requests.
- References held by callbacks, interrupts, timers, and I/O.
- Refusal to unload while code may still be reachable.
- Both x86-64 and AArch64 relocation paths.

Fuzzing of all untrusted parsers is required before production use.

## 15. Decisions still open

The following must be decided by separate design review before implementation is considered stable:

- Object/container format (ELF is only a candidate).
- Relocation subset per architecture.
- Exact DKM ABI versioning and interface namespace policy.
- Signature algorithm, key storage, enrollment, and revocation.
- Module memory layout and allocation strategy.
- Whether module-to-module dependencies are supported in the first release.
- Boot-time module transport and authentication.
- Whether unload is supported in the first release or initially restricted to selected module classes.

## 16. Completion criteria

DKM is not complete when a sample module merely loads. Completion requires a reviewed ABI, validated parser and relocation implementation, authenticated loading policy, transactional rollback, dependency handling, lifecycle/reference safety, architecture-specific tests, fuzzing, and documented compatibility guarantees.

**End of DKM specification draft.**
