# Wicofe Driver Framework (WDF) Specification

- **Document:** DRIVER_FRAMEWORK_SPEC.md
- **Version:** 0.1
- **Status:** Initial architecture and lifecycle contract
- **Applies to:** x86_64 and AArch64; Intel, AMD, and ARM platform/device ecosystems
- **License:** MPL-2.0 for original Wicofe documentation

## 1. Purpose

WDF is the native device and driver-management subsystem of the Wicofe monolithic kernel. It defines common device identity, driver matching, resource ownership, lifecycle, and integration contracts.

WDF is an original Wicofe implementation. It does not expose Linux kernel driver APIs and does not require Linux as a runtime dependency.

## 2. Native implementation policy

1. Wicofe driver code is written for Wicofe kernel interfaces.
2. Vendor specifications, public programming manuals, standards, and permissively licensed reference implementations may inform behavior.
3. Linux drivers may be studied as technical references, but Linux kernel APIs and implementation code are not ported as the Wicofe driver interface.
4. Reverse engineering is a last resort when authoritative documentation and lawful interoperability information are insufficient.
5. Every imported reference or third-party source must undergo license, provenance, and notice review before any code is incorporated.
6. Firmware blobs remain separate artifacts with their own provenance, redistribution, and licensing review. An open-source driver does not imply open-source firmware.

## 3. Scope and target matrix

Initial CPU architectures:
- x86_64, primarily PC and server platforms using PCI/PCIe and ACPI.
- AArch64, including systems using Device Tree, ACPI, PCI/PCIe, and platform buses.

Vendor/device families are targets, not a claim that every product generation is supported:
- Intel: integrated/discrete graphics, platform/chipset devices, network, audio, and other documented devices.
- AMD: Radeon/APU graphics, platform/chipset devices, network, audio, and other documented devices.
- ARM ecosystem: GIC, generic timer, SMMU, and platform IP; Mali and other SoC GPUs are separate device-family drivers. SoC integration may be controlled by silicon vendors.

ARM is both a CPU architecture ecosystem and an IP supplier. WDF must not assume that every AArch64 system uses the same interrupt controller, GPU, bus topology, or firmware description.

## 4. Layering and dependency direction

The intended layering is:

1. Hardware and firmware description.
2. Architecture/platform HAL.
3. Bus and platform enumeration.
4. Kernel resource services (MMIO, interrupts, DMA, IOMMU, power).
5. WDF device core and driver lifecycle.
6. Device-class subsystems (storage, network, audio, input, display, graphics).
7. Kernel device interfaces and userspace-facing ABI.

Dependencies point downward through documented interfaces. Device drivers must not bypass resource ownership to program arbitrary hardware addresses or register unmanaged interrupt handlers.

WDF is not the HAL. HAL provides architecture/platform primitives; WDF coordinates devices, drivers, and their resources.

## 5. Core concepts

- **Device:** A discovered hardware or virtual device with stable kernel identity and owned resources.
- **Driver:** A native implementation that supports one or more device identities/capabilities.
- **Bus:** A discovery and transport domain such as PCI/PCIe, USB, or platform bus.
- **Resource:** A bounded MMIO range, interrupt, DMA mapping, I/O region, or platform resource granted to a device.
- **Binding:** The association between a device and a successfully probed driver.
- **Device class:** A higher-level function such as block storage, network, audio, input, display, or graphics.

Device identity must be derived from trusted discovery data and validated before driver matching. Vendor/device identifiers alone are insufficient for all buses and platforms; ACPI IDs, Device Tree compatible strings, PCI class codes, subsystem IDs, and revision/capability data may also participate.

## 6. Driver lifecycle

The lifecycle contract is:

1. Discovered: bus/platform code has created a validated device record.
2. Matched: a candidate driver has been selected by declared match rules.
3. Probed: the driver has confirmed that it supports the device without publishing a running device.
4. Initialized: required resources have been acquired and device initialization has completed.
5. Running: device operations may be published to dependent kernel subsystems.
6. Stopped: new operations are blocked and in-flight work has been quiesced.
7. Removed: registrations and resources have been released.
8. Failed: an operation failed; acquired resources must be rolled back before the device can be retried or removed.

The implementation must define allowed transitions and rollback behavior. Removal is not permitted while callbacks, interrupts, DMA, or in-flight I/O can still reach the driver.

## 7. Resource ownership

Resources are granted by kernel managers and tracked against a device/driver binding.

- MMIO mappings must be range-checked and use device-appropriate memory attributes.
- Interrupt registrations must be masked/unregistered and synchronized before driver code or state is released.
- DMA mappings must be established through the DMA/IOMMU layer and revoked only after device access is stopped.
- I/O and bus resources must not overlap conflicting ownership.
- Power transitions must coordinate with active I/O and dependent devices.
- Partial initialization must unwind in reverse acquisition order.

No device driver may assume physical RAM is DMA-safe, that CPU and device addresses are identical, or that all platforms are cache coherent.

## 8. Hardware discovery

WDF consumes normalized device descriptions from bus/platform enumerators.

Required discovery paths:
- PCI/PCIe enumeration and configuration-space access.
- ACPI device and resource descriptions.
- Device Tree nodes and resource descriptions.
- USB enumeration after a host-controller driver is operational.
- Platform-specific buses where required by SoC integration.

ACPI and Device Tree are complementary discovery mechanisms. Their parsed data must be validated and normalized before it is exposed to drivers.

## 9. Architecture and platform boundaries

Architecture-specific operations remain behind HAL interfaces. Initial CPU targets are x86_64 and AArch64.

Examples:
- x86_64 interrupt delivery may involve APIC and MSI/MSI-X support.
- AArch64 interrupt delivery commonly involves GIC, but the exact version and integration are platform-dependent.
- DMA coherency and IOMMU support differ across platforms.
- Cache maintenance and memory barriers must be provided by architecture/platform services.

No driver may infer platform behavior solely from the CPU architecture name.

## 10. DKM relationship

WDF must support built-in drivers first. DKM-backed drivers may be considered only after the DKM ABI, loader, signing/trust policy, resource lifetime, callback quiescence, and safe-unload contracts are implemented.

The initial WDF API is an in-kernel Rust interface, not a stable DKM ABI. Rust trait layout, Rust references, and trait objects must not cross a future DKM boundary. A versioned C-compatible descriptor/function-table ABI requires a separate design and review.

## 11. Userspace and ABI boundary

WDF is kernel-internal. Userspace interacts through explicitly defined Wicofe device interfaces, VFS/device nodes, and versioned syscall ABI contracts.

Driver implementation details, Rust types, and kernel pointers must never be exposed as userspace ABI. Device permissions and ownership are enforced by kernel security/VFS policy, not by arbitrary driver-specific conventions.

## 12. Initial implementation stages

1. Audit and document existing kernel prerequisites.
2. Define device identity, lifecycle states, errors, and internal driver contracts.
3. Implement lifecycle transition validation and unit tests.
4. Implement a bounded Device Manager registry with explicit capacity/error behavior.
5. Add mock drivers and matching/binding tests.
6. Integrate resource ownership interfaces after MMIO/IRQ/DMA managers are ready.
7. Add PCI/ACPI/Device Tree enumeration.
8. Add basic platform, storage, USB, and network drivers.
9. Add display, audio, and graphics families, including Intel, AMD, and ARM targets.
10. Add DKM driver support only after the separate DKM prerequisites are met.

The stages may overlap only when dependencies are explicit and testable.

## 13. Current kernel constraints

At the initial WDF baseline:
- The kernel is a Rust `no_std` configurable monolithic kernel.
- x86_64 and AArch64 are compile targets; AArch64 runtime boot validation remains pending.
- PMM bootstrap is present but remains single-core and is not enabled for general kernel allocations.
- Live VMM activation, kernel heap, SMP synchronization, and complete interrupt management are not yet available.
- DKM is a draft specification, not an implemented loader or stable module ABI.

Therefore, the first WDF implementation must remain allocation-free and hardware-independent. It must not map MMIO, register hardware interrupts, submit DMA, or claim real device enumeration until the corresponding kernel services exist.

## 14. Testing and completion criteria

Each stage requires:
- Unit tests for identity validation, matching, lifecycle, and error paths.
- Capacity/exhaustion tests for bounded registries.
- Rollback tests for partial initialization.
- Architecture-specific compile checks.
- Emulator tests when the relevant hardware model is available.
- Physical-hardware tests before claiming device support.
- Documentation distinguishing implemented, compiled, emulator-tested, and hardware-tested behavior.

A successful compile alone does not establish hardware support.

**End of WDF specification baseline.**
