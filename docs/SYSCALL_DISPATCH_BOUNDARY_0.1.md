# Syscall Dispatch Boundary 0.1

- **Status:** Implemented architecture-neutral request/result interface; architecture entry and runtime dispatch are deferred.
- **Scope:** Kernel-side boundary between architecture register decoding and syscall policy.
- **Architectures:** x86-64 and AArch64 are intended targets.

## 1. Current implementation

`kernel/src/syscall.rs` provides:

- `SyscallRequest`: syscall number plus six arguments in ABI order.
- `SyscallDispatcher`: kernel policy interface that receives a decoded request and returns a logical outcome.
- `dispatch_request`: maps a dispatcher outcome to the adopted architecture-neutral result representation.
- `SyscallReturn`: machine-word value plus a carry discriminator. Architecture return code must translate this to x86-64 RFLAGS.CF or AArch64 PSTATE.C.

The request intentionally contains no CPU register names. The x86-64 and AArch64 entry adapters must decode their respective register conventions before constructing it.

## 2. Security requirements for implementations

A dispatcher implementation must:

1. Validate every user pointer, length, alignment, and address range before dereferencing user memory.
2. Use a defined copy-in/copy-out mechanism that handles faults; a raw pointer cast is not validation.
3. Check integer arithmetic for overflow when computing user ranges.
4. Avoid retaining user virtual addresses after the syscall operation.
5. Validate file descriptors, handles, object ownership, and permissions in the relevant subsystem.
6. Return success values with carry clear, including negative signed values represented in the machine word.
7. Return a positive, non-zero Wicofe errno with carry set on failure.

The trait is an interface only. It does not implement user-memory access, process isolation, authorization, scheduling, or a syscall table.

## 3. Numbering policy

No syscall numbers are assigned by this milestone. Numbers must be allocated through the canonical Wicofe-ABI syscall registry before architecture entry code or userspace stubs depend on them. Unknown numbers must eventually produce the ABI-defined `ENOSYS` result.

## 4. Deferred architecture work

The following remain unimplemented:

- x86-64 SYSCALL MSR configuration, entry assembly, per-CPU kernel stack selection, and SYSRETQ/IRETQ return policy.
- AArch64 SVC vector setup, exception-level validation, per-CPU stack selection, and ERET return policy.
- Full user register frames, including user instruction pointer, stack pointer, status flags, and privilege state.
- Per-thread syscall context, process/address-space association, and safe user-copy primitives.
- A concrete syscall registry and subsystem handlers.
- Signal delivery, interruption, restart, and cancellation semantics.
- QEMU tests that execute a syscall from ring 3 / EL0 and verify returned value and carry state.

The existing x86-64 IDT exception/IRQ frame is not reused as a syscall frame: its current same-privilege exception layout does not model the complete user return context required by SYSRETQ or IRETQ.

## 5. Validation status

Unit tests cover request preservation and mapping of success/error outcomes. They do not prove architecture entry correctness or user/kernel isolation. Runtime syscall support must not be claimed until the deferred entry, memory-safety, and QEMU tests are complete.
