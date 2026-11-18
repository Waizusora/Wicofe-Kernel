# Wicofe Kernel ABI Boundaries

- **Status:** Design baseline
- **Version:** 0.1
- **Applies to:** Wicofe-Kernel, Wicofe-ABI, Wicofe-Drivers, Wlibc
- **License:** MPL-2.0 for original project material

## 1. Purpose

Wicofe has two distinct ABI boundaries. They must not be conflated:

1. **Userspace syscall ABI**, owned by `Wicofe-ABI`. It defines the contract between user processes and the kernel.
2. **Dynamic Kernel Module ABI (DKM ABI)**, owned by `Wicofe-Kernel`. It defines the contract between the running kernel and privileged in-kernel modules.

These interfaces have different threat models, compatibility requirements, and lifecycles. A userspace syscall ABI does not load or describe kernel modules.

## 2. Ownership

| Interface | Canonical owner | Implementers / consumers |
|---|---|---|
| Syscall identifiers, calling convention, arguments, errors | Wicofe-ABI | Kernel syscall layer, Wlibc, Rust userspace runtime |
| User-visible structures and constants | Wicofe-ABI | Kernel, Wlibc, Wicofe-Lib, SDK |
| DKM metadata, loader contract, exported kernel function tables | Wicofe-Kernel | Kernel module manager and Wicofe-Drivers |
| VFS internal interfaces | Wicofe-Kernel | Built-in filesystems and DKM filesystem modules |
| ONWFS disk format | ONWFS | Kernel ONWFS implementation and offline tools |

No consumer repository may independently assign syscall numbers or redefine a public structure.

## 3. Userspace syscall ABI rules

The syscall ABI is a public compatibility surface. Its specification must define, for each call:

- Stable symbolic name and unique numeric identifier.
- Supported architectures and architecture-specific entry instruction.
- Register/stack argument placement and return-value rules.
- Width, signedness, alignment, and pointer semantics of every argument.
- Error representation and mapping to libc `errno`.
- Blocking, interruption, cancellation, and restart behavior where relevant.
- Permission checks and observable side effects.
- Introduction version, stability level, and deprecation policy.

Use fixed-width integer types and explicitly defined C-compatible structures at the boundary. Do not expose Rust compiler-private layout, references, slices, `String`, `Vec`, trait objects, or Rust enum representation as public ABI.

Pointers supplied by userspace are untrusted. The kernel must validate ranges, alignment, access permissions, arithmetic overflow, and object lifetime before use. Copying data across the boundary must follow documented fault and partial-transfer semantics.

The native C runtime is **Wlibc**, which targets the declared ISO C/POSIX profile and consumes this ABI. It does not define a parallel syscall ABI. POSIX interfaces may be composed from multiple syscalls or implemented in a separate userspace POSIX layer; only actual kernel mechanisms belong in this ABI.

## 4. DKM ABI rules

A DKM executes with kernel privilege. ABI validation prevents accidental incompatibility; it does not sandbox a malicious or defective module.

The DKM boundary must:
- Use an explicitly versioned C-compatible representation.
- Use fixed-width fields, opaque handles, and function tables.
- Define ownership, lifetime, mutability, and synchronization for every handle and callback.
- Avoid Rust ABI, trait-object layout, compiler-private types, and unwinding across the boundary.
- Expose only intentionally supported kernel interfaces.
- Validate architecture, kernel ABI version, module dependencies, and required capabilities before initialization.
- Prevent unload until callbacks, interrupt handlers, device bindings, work items, and executing code are quiescent.

DKM metadata and relocation format remain kernel design decisions. Do not assume ELF or a specific relocation set until a separate decision is recorded.

## 5. Versioning and compatibility

Every ABI surface must have an explicit version and compatibility policy. Structure extension must be designed, not inferred from compiler layout.

For extensible structures, prefer a declared size/version field and reserved fields with documented input/output rules. Unknown mandatory features must cause a defined compatibility error. Unknown optional capabilities may be ignored only when the contract explicitly permits it.

Kernel releases must publish the userspace ABI version/capability set and DKM ABI version/capability set independently.

## 6. Error and failure behavior

Every interface must define stable error semantics. Do not expose Rust `Result`, panic payloads, or enum discriminants as ABI values.

A syscall failure must be distinguishable from a valid return value according to the selected syscall convention. The exact raw return convention and errno mapping are open decisions for Wicofe-ABI.

A DKM load failure must leave no partially published module, dangling callback, leaked registration, or unresolved dependency. Failure to prove safe unload means the module remains loaded.

## 7. Required review before stabilization

Before either ABI is declared stable, require:
- A written architecture-specific calling convention.
- Machine-readable or generated definitions where practical.
- Layout and alignment tests for public structures.
- Compatibility tests across supported kernel and userspace versions.
- Negative tests for malformed pointers, metadata, versions, and unsupported capabilities.
- Documentation of every breaking-change and deprecation rule.

**This document records boundaries and invariants. It does not freeze syscall numbers, register assignments, DKM object format, or relocation types.**
