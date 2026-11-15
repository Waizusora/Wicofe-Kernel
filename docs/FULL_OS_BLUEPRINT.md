# WICOFE Full Operating System Blueprint

- **Project:** WICOFE OS
- **Document:** FULL_OS_BLUEPRINT.md
- **Version:** 0.1
- **Status:** System architecture blueprint
- **Kernel baseline:** Configurable monolithic Rust `no_std`
- **Original project license:** MPL-2.0
- **Policy:** No GNU GPL, LGPL, or AGPL dependencies in the supported default system

> This document is a roadmap and architecture plan. It does not claim that the listed subsystems are implemented. Each component must have its own specification, repository ownership, implementation milestones, and license audit.

## 1. Product objective

WICOFE is intended to be a complete, independently usable operating system, not merely a kernel. The default installation should provide a coherent, ready-to-use system: boot, kernel, drivers, filesystem, init/service management, shell, command-line utilities, networking, graphical desktop, applications, installer, update/recovery tools, and developer documentation.

At the same time, WICOFE must be modular. Experienced OS builders should be able to select, replace, or omit components and use compatible WICOFE components in another operating system without requiring the complete WICOFE distribution.

Design principles:

1. Complete and usable by default.
2. Modular and replaceable by design.
3. Monolithic kernel, with selected kernel components optionally supplied as DKM.
4. Rust-first implementation; `no_std` for the kernel and normal Rust `std` for hosted user-space programs where appropriate.
5. Explicit, versioned interfaces between kernel, user space, services, and desktop.
6. No copied source code from Linux distributions or desktop environments.
7. License and provenance audit for every direct and transitive dependency.

## 2. Existing accepted architecture decisions

| Area | Current decision |
|---|---|
| Kernel architecture | Configurable monolithic kernel |
| Kernel language | Rust `no_std`, Assembly for architecture-specific operations |
| Initial CPU architectures | x86-64 and AArch64 |
| Boot | Limine / Limine Boot Protocol |
| Kernel modules | DKM, loaded in kernel space; does not change monolithic architecture |
| Memory management | Custom Hybrid: early bitmap, buddy PMM with bitmap/frame metadata, VMM, slab/size-class kernel heap |
| Native filesystem | ONWFS — Oh Nice WICOFE Filesystem |
| Additional built-in filesystem | FAT32 |
| Other filesystems | Optional DKM when boot and dependency constraints permit |
| Shell | Mysh |
| Basic command-line utilities | uutils/coreutils, subject to exact dependency and license audit |
| System initialization | Osynit — Open System Initialize (planned WICOFE init/system manager) |
| Desktop | WDE — Wicofe Desktop Environment |
| Original project code | MPL-2.0 |
| Dependency policy | Exclude GPL, LGPL, AGPL from supported default system |

NFS is a network filesystem protocol, not a local disk filesystem. If supported, it belongs in the network filesystem layer and may be delivered as an optional filesystem module after the network stack and DKM interfaces are ready.

## 3. System layers

The system is divided into the following layers. These are logical boundaries; they do not all need separate repositories on day one.

### Layer A — Firmware and boot

- UEFI boot path.
- Limine bootloader and protocol integration.
- Boot configuration and entry selection.
- Kernel image loading.
- Boot module/initramfs loading.
- Secure Boot/signature policy (design and implementation milestone).
- Normalized boot handoff contract.
- Recovery boot entry.

Repository/component: WICOFE kernel repository for protocol integration and boot specifications; separate boot assets/configuration repository only when packaging requires it.

### Layer B — Kernel

Repository: `Waizusora/WICOFE`

Built-in kernel foundations:
- Architecture entry and HAL.
- CPU and SMP initialization.
- Physical memory manager (PMM).
- Virtual memory manager (VMM).
- Kernel heap.
- Interrupt and exception handling.
- Timer subsystem.
- Scheduler and context switching.
- Process and thread management.
- System-call ABI.
- IPC and synchronization.
- Security and access control.
- VFS.
- Block I/O and device model.
- DKM manager and module ABI.
- Kernel logging and diagnostics.

Configurable component modes remain `built-in`, `module`, and `disabled`. Components needed before root filesystem mounting must be built in or available through a defined boot-time module path.

### Layer C — Drivers and hardware support

Initial driver families, ordered by boot and desktop needs:

1. Firmware/platform and interrupt controllers.
2. PCI/PCIe enumeration.
3. Storage controllers and block devices.
4. USB host controllers and USB device framework.
5. Input: keyboard, mouse, touchpad, touchscreen.
6. Display/GPU and framebuffer/DRM-like abstraction.
7. Network: Ethernet and Wi-Fi.
8. Audio.
9. Bluetooth.
10. Power, battery, thermal, sensors, camera, and other optional devices.

Drivers may be built-in or DKM. Storage, bus, and filesystem drivers needed to mount the root filesystem must be available early enough. A DKM is privileged kernel code and must be authenticated and ABI-compatible.

### Layer D — Filesystem and storage

Core:
- VFS and mount namespace.
- Block I/O interface.
- Partition table support (GPT first; MBR compatibility as required).
- ONWFS native filesystem.
- FAT32 interoperability filesystem.
- Mount, fsck/check, formatting, and recovery tools.
- Swap/pagefile policy if adopted.
- Storage discovery and volume identity.

ONWFS goals:
- Inode-based hierarchy.
- Directories, symlinks, hard links.
- UID/GID and POSIX-style permissions.
- Timestamps and extended attributes.
- Sparse files.
- Metadata checksums.
- Journaling and crash recovery.
- Versioned on-disk format and feature flags.

The ONWFS disk format is still a separate design decision and must be finalized before stable data is written. FAT32 does not natively store full POSIX ownership, permissions, symlinks, or hard links; WICOFE must report these limitations honestly.

Optional filesystem implementations:
- NFS client (network filesystem).
- Other local/network filesystems through DKM if their license, security, and ABI requirements pass review.

### Layer E — User-space runtime and ABI

A complete OS needs a stable user-space contract, not just a kernel syscall table.

Required components:
- System call ABI and userspace headers.
- Process startup/loader support.
- ELF executable loader (format and ABI specification).
- Dynamic linker/loader if dynamic linking is selected.
- C runtime/libc strategy.
- Rust target/runtime support.
- Threading and synchronization runtime.
- Signals, file descriptors, pipes, sockets, and process environment.
- Locale and time interfaces.
- POSIX compatibility profile, documented by implemented feature level.

A musl-based libc is a candidate for a non-GNU user-space C runtime. Rust `std` support must be treated as a separate target-porting effort; it does not appear automatically merely because the kernel is written in Rust.

### Layer F — System initialization and services

**Osynit — Open System Initialize**

Planned responsibility:
- PID 1 / first userspace process.
- Early userspace initialization.
- Mounting and preparation of required virtual filesystems.
- Device/service readiness coordination.
- Starting and supervising system services.
- Service dependency ordering.
- Shutdown and reboot orchestration.
- Logging integration.
- User session startup handoff.

Potential later capabilities:
- Service unit format.
- Restart policies and health state.
- Socket activation.
- Timers.
- Resource limits.
- User-level service sessions.

Osynit should remain independent of WDE. WDE's session manager is responsible for the desktop session; Osynit manages system initialization and system services. Avoid duplicating service supervision between them.

### Layer G — Command-line environment

**Mysh** is the default WICOFE shell.

Expected shell capabilities:
- Interactive command line.
- Command execution and process creation.
- Pipelines and redirection.
- Environment variables.
- Quoting and expansion.
- Shell scripting.
- Job control, when process-group and terminal support is ready.
- Completion and history.
- Shell configuration.

**uutils/coreutils** is the planned default basic utility collection, subject to a full license and feature audit. It should be packaged as individual commands so users can replace or omit utilities. Do not assume every uutils utility or transitive dependency automatically passes the WICOFE policy.

Additional userland utilities required for a usable system:
- File and directory tools.
- Text processing.
- Archive/compression tools.
- Process and system inspection.
- Disk/partition and filesystem tools.
- Network diagnostics and configuration.
- User and permission management.
- Package management.
- Editor and pager.
- System logging inspection.
- Hardware information and diagnostics.

Avoid introducing GNU-specific implementations or GNU GPL utilities into the default base system.

### Layer H — Networking

Required components:
- Network device API and drivers.
- Ethernet and Wi-Fi support.
- Link management.
- IPv4 and IPv6.
- ARP and Neighbor Discovery.
- DHCP client.
- DNS resolver.
- Routing and firewall policy.
- Socket API.
- TCP and UDP.
- TLS implementation and certificate store.
- Network configuration service and CLI.

Later/optional:
- VPN.
- NFS client.
- Bluetooth networking.
- Advanced firewall and traffic control.

Networking should be split into kernel mechanisms and userspace policy/services. The exact division must be specified before implementation.

### Layer I — Graphics and desktop

**WDE — Wicofe Desktop Environment**

Repository: `Waizusora/Wicofe-WDE`

WDE is a separate, modular desktop environment. Its default appearance is intentionally simple and lightweight.

Default visual specification:
- Solid black or white desktop background.
- One bottom panel, always visible by default.
- No top dock.
- No desktop shortcuts by default.
- No desktop widgets by default.
- No blur or transparency.
- No unnecessary animation.
- Window controls are small circles, left to right:
  - Red: close.
  - Yellow: maximize/restore.
  - Green: minimize.

WDE components:
- Wayland compositor and window manager.
- Window decoration and input handling.
- Panel and task list.
- Application launcher.
- Desktop session.
- Settings.
- Theme system.
- Display configuration.
- Notification service integration (later, optional).
- Accessibility interfaces.

WDE must not depend on Osynit internals. It should integrate through documented session/service interfaces and remain usable in compatible standalone Linux environments.

Wayland is the display protocol. Smithay is the current compositor framework candidate; its exact dependency graph and enabled feature licenses must be audited. Smithay supplies low-level compositor helpers, not WDE's complete window-management policy or visual design.

### Layer J — Audio, multimedia, and peripherals

Required for a full desktop experience:
- Audio server/service architecture.
- ALSA-compatible hardware access strategy or an independently implemented equivalent.
- Device discovery and routing.
- Volume controls and mixer API.
- Media playback APIs.
- USB audio.
- Camera and video capture interfaces where supported.

The initial audio architecture should be small and modular; advanced multimedia can be optional packages.

### Layer K — Applications and developer tools

Default base applications:
- Terminal using Mysh.
- File manager.
- Text editor.
- Settings application.
- System monitor.
- Archive manager.
- Image viewer.
- Basic document/PDF viewer where licensing permits.

Developer toolkit:
- Rust toolchain target support.
- C compiler/toolchain strategy.
- Assembler and linker.
- SDK headers and libraries.
- Debugger and symbol tools.
- Build system and package recipes.
- Sysroot and cross-compilation support.
- API/ABI documentation.

### Layer L — Package management and software distribution

A complete OS needs:
- Package format and metadata schema.
- Repository metadata and signing.
- Dependency resolution.
- Install/remove/upgrade/rollback.
- File ownership database.
- Conflict handling.
- Package scripts/hooks policy.
- Reproducible build metadata.
- Offline/local package installation.
- Repository mirror support.

Package manager and package format are not yet selected. Keep them independent of Mysh and Osynit through stable CLI and service interfaces.

### Layer M — Installer, updates, and recovery

Required:
- Bootable installation media.
- Hardware and disk discovery.
- Partitioning and formatting.
- Base-system installation.
- Bootloader installation/configuration.
- User creation.
- Network setup.
- Desktop selection (default WDE).
- Update mechanism.
- Recovery environment.
- Filesystem check and repair tools.
- System rollback strategy where supported.

The installer should support a default complete WICOFE installation and a component-selection mode for advanced users.

### Layer N — Security and system policy

Required:
- User/group database.
- Authentication and login.
- Permission and ownership model.
- Privilege escalation mechanism.
- Service identity and sandboxing strategy.
- DKM signature verification.
- Secure update verification.
- Firewall policy.
- Certificate store.
- Audit/event logging.
- Security update process.

Do not treat a monolithic kernel or DKM ABI validation as a security sandbox. DKM executes with kernel privilege.

### Layer O — Documentation, testing, and release engineering

Required:
- System architecture documentation.
- Kernel and syscall ABI documentation.
- Driver and DKM developer documentation.
- ONWFS format and recovery specification.
- User and administrator manuals.
- Package maintainer guide.
- SDK and porting guide.
- Hardware compatibility list.
- Automated CI.
- Emulator tests (QEMU first).
- Boot/install/upgrade/recovery tests.
- Filesystem fault-injection tests.
- Security and dependency license audit.
- Release notes and versioning policy.

## 4. Recommended repository map

Do not create every repository immediately. Start with clear ownership and split only when a component has a stable independent build, interface, and release cadence.

| Repository | Responsibility | Status |
|---|---|---|
| `Waizusora/WICOFE` | Kernel, HAL, VFS, DKM, boot integration, core driver interfaces | Exists |
| `Waizusora/Wicofe-WDE` | Wayland desktop environment | Exists |
| `Waizusora/Mysh` | WICOFE default shell | Existing project; verify current repository name |
| `Waizusora/uutils` or upstream uutils | Basic userland utilities packaging/integration | Integration project; avoid unnecessary fork |
| `Waizusora/Osynit` | Init and system service manager | Planned; repository not yet confirmed |
| `Waizusora/ONWFS` | Native filesystem format and implementation | Planned; repository not yet confirmed |

Potential future independent repositories, only when justified:
- WICOFE-libc / userspace runtime.
- WICOFE-network.
- WICOFE-package-manager.
- WICOFE-installer.
- WICOFE-audio.
- WICOFE-SDK.
- WICOFE-ports (package recipes and third-party source metadata).

The kernel repository should not absorb every userspace application. Likewise, WDE should remain separate from the kernel and Osynit.

## 5. Default installation profile

The standard WICOFE image should be a ready-to-use OS containing:

- Limine boot path.
- WICOFE monolithic kernel.
- Required built-in drivers.
- VFS, ONWFS and FAT32 support.
- Osynit.
- User-space runtime and libc.
- Mysh.
- uutils/coreutils utilities that pass license review.
- Networking services.
- User/login management.
- Package manager.
- WDE desktop.
- Basic graphical applications.
- Installer, updater, and recovery tools.

Advanced installation should allow:
- Minimal CLI-only profile.
- Standard desktop profile.
- Developer profile.
- Custom component selection.
- Optional DKM drivers and filesystems.

The default image is complete, but components should not become inseparable.

## 6. Implementation order and dependency gates

### Phase 0 — Contracts and repository foundations
- Freeze system architecture and component ownership.
- Define kernel/userspace boundary.
- Define syscall and process ABI strategy.
- Define DKM ABI and boot-time module policy.
- Establish license inventory and CI policy.
- Publish cross-repository versioning rules.

### Phase 1 — Bootable kernel foundation
- Limine handoff and architecture entry.
- x86-64 and AArch64 bring-up.
- HAL, early memory, PMM and VMM.
- Interrupts, timer, scheduler.
- Kernel heap and diagnostics.
- QEMU boot tests.

### Phase 2 — Process and storage foundations
- User-mode transition.
- ELF loader.
- Syscall ABI.
- Process/thread lifecycle.
- Block I/O and device model.
- VFS core and root mount path.

### Phase 3 — First usable command-line OS
- ONWFS format finalized and initial implementation.
- FAT32 support.
- User-space runtime/libc.
- Osynit minimal PID 1.
- Mysh.
- Initial uutils command set.
- Basic file, process, and system tools.

### Phase 4 — Hardware and networking
- Storage and USB drivers.
- Keyboard/mouse.
- Display and GPU foundation.
- Ethernet/Wi-Fi.
- IP, DHCP, DNS, sockets.
- Audio foundation.

### Phase 5 — DKM and optional subsystems
- Versioned DKM ABI.
- Object format and relocation policy.
- Module signing and trust policy.
- Driver and filesystem module loading.
- NFS client as optional network filesystem.

### Phase 6 — Complete desktop
- Wayland compositor.
- WDE window management and decorations.
- Persistent bottom panel.
- Launcher and settings.
- Display, input, network, audio integration.
- Desktop session startup through Osynit handoff.

### Phase 7 — Distribution
- Package manager and signed repositories.
- Installer and live/recovery image.
- Default application set.
- Updates and rollback.
- Documentation and release engineering.

### Phase 8 — Stabilization
- ABI compatibility policy.
- Hardware compatibility expansion.
- Security review.
- Performance and reliability testing.
- Stable release and long-term maintenance policy.

## 7. License and provenance policy

The WICOFE original-code baseline is MPL-2.0. The supported default distribution excludes GNU GPL, LGPL, and AGPL dependencies according to the project's current policy.

Permissive candidates include MIT, BSD-2-Clause, BSD-3-Clause, Apache-2.0, ISC, Zlib, and 0BSD, subject to exact compatibility and notice review.

Rules:
1. Audit exact versions and all transitive dependencies.
2. Audit optional features and target-specific dependencies.
3. Audit bundled fonts, icons, firmware, protocol definitions, and generated code separately.
4. Preserve third-party notices.
5. Do not infer a dependency's license from its parent project.
6. A project being MPL-2.0 does not make its dependencies MPL-2.0.
7. If a required component has an excluded license, do not include it in the supported default system; select or develop an acceptable alternative.

## 8. Open decisions that must not be silently assumed

- Exact user-space syscall ABI and POSIX compatibility target.
- ELF ABI details and dynamic-linking strategy.
- libc implementation and Rust `std` target strategy.
- ONWFS on-disk format and journal mode.
- DKM binary format, relocation subset, and signing trust model.
- Initial hardware support matrix.
- Network stack kernel/userspace split.
- Audio server architecture.
- Package format and package manager.
- Login/authentication and privilege escalation design.
- Installer partitioning and rollback strategy.
- Secure Boot key ownership and update policy.

## 9. Definition of a complete WICOFE OS

WICOFE should not be called a complete OS merely because the kernel boots.

A usable complete release must demonstrate, on documented target hardware or emulator:

1. Boots from firmware through Limine into WICOFE.
2. Initializes memory, interrupts, scheduler, and required drivers.
3. Mounts a root filesystem and survives reboot safely.
4. Starts Osynit as PID 1.
5. Starts a user login/session.
6. Provides Mysh and a usable set of command-line utilities.
7. Creates, reads, writes, and manages files.
8. Supports networking on declared hardware.
9. Installs, removes, and updates packages.
10. Starts WDE and runs graphical applications.
11. Shuts down and reboots cleanly.
12. Provides recovery and diagnostic paths.
13. Passes dependency license and security checks for the shipped image.

**Project principle:** A complete operating system by default; independently reusable components by design.
