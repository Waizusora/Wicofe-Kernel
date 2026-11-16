# Osynit PID 1 — Kernel Requirements 0.1

- **Status:** Proposed cross-component requirements
- **Owner:** Wicofe-Kernel for kernel mechanisms; Osynit for userspace policy
- **Date:** 2026-11-16
- **Normative coordination:** [Wicofe-Specs Osynit boot/service contract](https://github.com/Waizusora/Wicofe-Specs/blob/main/boot/OSYNIT_BOOT_AND_SERVICE_CONTRACT_0.1.md)

## 1. Boundary

Osynit is the Wicofe userspace initializer and PID 1 service lifecycle supervisor. The kernel provides mechanisms; Osynit implements policy and orchestration. This document does not assign syscall numbers or freeze ABI layouts.

## 2. Required kernel mechanisms

The kernel/ABI combination must eventually provide documented, testable semantics for:

1. **Initial process handoff:** start the configured init executable as PID 1 with defined argv, environment, credentials, working directory, file descriptors, and initial mount/namespace assumptions.
2. **Process lifecycle:** create/execute processes, observe exit status, wait/reap exited children, and define orphan adoption/reparenting behavior.
3. **Signals and notifications:** deliver process signals, define interruption/restart behavior for wait operations, and ensure child-exit notifications cannot be silently lost.
4. **Process identity:** define PID lifetime and reuse semantics; provide a safe way to avoid signaling a different process after PID reuse. A pidfd-style facility is optional if an equivalent safe contract exists.
5. **File descriptors and VFS:** provide the file operations and descriptor semantics required by Osynit's service execution and IPC.
6. **Local IPC:** provide Unix-domain sockets or an explicitly specified equivalent local transport. If `SOCK_SEQPACKET` is unavailable, Osynit may use a bounded, versioned stream framing protocol.
7. **Time:** provide monotonic clock semantics and timeout behavior for supervision, restart backoff, jobs, and shutdown.
8. **System transition:** provide a privileged, documented reboot/poweroff/halt mechanism with defined authorization and error behavior.
9. **Capability discovery:** expose which optional platform facilities are implemented so Osynit does not infer Linux compatibility from target architecture.

## 3. PID 1 correctness invariants

- PID 1 must be able to reap every waitable child.
- Orphan adoption semantics must be explicit and testable.
- Signal delivery and wait semantics must remain correct under bursts of child exits.
- A stale PID must never be treated as proof of the original process identity.
- Blocking operations used in the event loop must have bounded or interruptible behavior.
- Kernel failure to provide a required primitive must be reported as a platform conformance gap, not hidden by a userspace fallback that changes semantics.

## 4. Explicit non-requirements

The kernel does not implement service definitions, dependency graphs, boot targets, restart policies, service configuration parsing, Osynit CLI commands, logging policy, or domain daemons. Those are userspace responsibilities.

Cgroups, pidfds, Linux-specific signal flags, and Linux reboot syscall numbers are not assumed. Each capability must be specified independently for Wicofe.

## 5. Verification requirements

Kernel/ABI conformance tests should cover initial PID 1 handoff, fork/exec/exit/wait lifecycle, multiple simultaneous child exits, orphan reparenting, signal interruption, PID reuse safety, socket framing/closure, monotonic timeout behavior, and privileged power-transition authorization.

Host tests of Osynit logic do not establish native Wicofe PID 1 conformance or bootability.
