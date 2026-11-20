# Frenit Integration Boundary

- **Status:** Integration contract proposed; native PID 1 handoff not implemented or verified.
- **Canonical init project:** [Frenit — Free Init](https://github.com/Waizusora/Frenit).
- **Cross-component contract:** [WIC-SPEC-BOOT-FRENIT-001](https://github.com/Waizusora/Wicofe-Spec/blob/main/boot/FRENIT_INTEGRATION_CONTRACT_0.1.md).
- **Owners:** Frenit (init userspace), Wicofe-Kernel (kernel mechanisms), Wicofe-ABI (syscall ABI), Wicofe-Test (conformance).

## 1. Canonical identity

Wicofe uses **Frenit (Free Init)** as its init-system project. The repository is `Waizusora/Frenit`; the Cargo package and executable are both `frenit`. The command convention is `frenit <domain> <action> [argument]`.

No previous init-system proposal, executable name, service schema, socket path, or implementation is part of this integration. Frenit repository documentation is authoritative for Frenit's own CLI and implementation.

## 2. Responsibility split

| Component | Responsibility |
|---|---|
| Frenit | PID 1 userspace executable, service lifecycle policy, targets, configuration, CLI and service orchestration |
| Wicofe-Kernel | Process creation/exec/exit, wait/reap, scheduling, signal delivery mechanisms, file descriptors, VFS/mount, clocks, IPC primitives, shutdown/power mechanisms |
| Wicofe-ABI | Stable public syscall numbers, register calling convention, data structures, error model and compatibility policy |
| Wicofe-Spec | Cross-component requirements and versioned integration contract |
| Wicofe-Test | Native boot, PID 1, process, signal, service and shutdown conformance tests |

Frenit must not duplicate kernel mechanisms or define private syscall numbers. Kernel code must not embed Frenit service policy.

## 3. Kernel boot boundary

The kernel must not select or execute Frenit as PID 1 until all of the following are implemented and tested:

1. A defined kernel-to-userspace handoff with a known entry point, initial stack, arguments/environment and credentials.
2. A root filesystem that contains the Frenit executable, required libraries, configuration and service metadata with validated permissions.
3. Process creation and `exec` semantics, including executable format validation and failure reporting.
4. PID identity, child exit status, wait/reap and signal delivery semantics sufficient for PID 1 responsibilities.
5. File descriptor, VFS/mount, clock and IPC interfaces required by Frenit's runtime.
6. A controlled shutdown/reboot/poweroff mechanism with authorization and failure behavior.
7. QEMU integration tests that prove Frenit becomes PID 1, starts a test service, reaps a child, handles a controlled service failure and completes a shutdown request.

Until these gates pass, the current kernel boot path remains in its existing diagnostic/fail-stop state. Do not add a speculative `init=/...` path or claim native PID 1 support.

## 4. ABI and compatibility rules

- The public syscall ABI is owned by [Wicofe-ABI](https://github.com/Waizusora/Wicofe-ABI); the kernel implements it.
- POSIX source compatibility does not imply Linux syscall-number or Linux binary-ABI compatibility.
- Do not import Linux-specific syscall assumptions into Frenit or kernel boot code.
- Kernel-internal process structures are not public ABI unless explicitly specified and versioned.
- Unimplemented privileged operations must return an explicit unsupported/error result; never report success.

## 5. Current implementation and verification

| Capability | Implementation | Verification |
|---|---|---|
| Frenit identity, crate and CLI foundation | Implemented in Frenit repository | Frenit CI is authoritative |
| Cross-repository ownership and boot gates | Proposed in Wicofe-Spec and documented here | Review required |
| Kernel initial userspace handoff | Not implemented | Not run |
| Native Frenit PID 1 startup | Not implemented | Not run |
| Native process/signal/service integration | Not implemented | Not run |
| Native shutdown integration | Not implemented | Not run |

## 6. Next kernel milestones

1. Extend the PIT-backed monotonic timebase into a documented clock interface; keep global interrupt enabling explicit.
2. Establish the kernel scheduler and single-process execution substrate.
3. Define and implement process creation, executable loading, exit and wait/reap against Wicofe-ABI.
4. Implement signal delivery and child-exit notification semantics.
5. Add VFS root mount and userspace descriptor support.
6. Implement the explicit initial-userspace handoff and only then add Frenit PID 1 QEMU tests.

Every milestone must have a kernel test and, where it crosses repository boundaries, a Wicofe-Test conformance case.
