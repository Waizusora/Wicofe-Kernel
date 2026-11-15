# WICOFE Boot Foundation Specification

- **Document:** BOOT_SPEC.md
- **Version:** 1.0
- **Status:** Initial boot architecture decision
- **Targets:** x86-64 and AArch64
- **Bootloader:** Limine
- **Kernel language:** Rust `no_std`

## 1. Purpose

This document defines the initial firmware-to-kernel boot contract and the boundary between the bootloader protocol and architecture-neutral WICOFE initialization. It is a specification and scaffold plan, not evidence of a bootable image.

## 2. Bootloader and protocol decision

WICOFE selects the **Limine Boot Protocol** and the Limine reference bootloader for the initial boot path on x86-64 and AArch64.

Reasons:
- The official Limine project lists x86-64 and AArch64 support.
- The protocol provides memory-map, higher-half direct-map, executable handoff, and other request/response features.
- It supports ELF as a recommended executable format, while not requiring ELF exclusively.
- The bootloader remains a replaceable external component; the kernel's normalized boot-information interface must not expose Limine-specific structures to general kernel subsystems.

The Limine bootloader project is BSD-2-Clause. The separate `limine-protocol` repository identifies its protocol specification/header as 0BSD. These are third-party components and retain their own licenses. WICOFE must preserve applicable notices and must not relicense either project. Before vendoring headers or source, record the exact repository, revision, file provenance, and license in the dependency inventory.

Official references:
- https://github.com/limine-bootloader/limine
- https://github.com/limine-bootloader/limine-protocol
- https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md

The protocol is 64-bit and little-endian. Its documented calling conventions are System V ABI on x86-64 and AAPCS64 on AArch64, with FP/SIMD disabled at the protocol boundary. The kernel entry code must obey the relevant architecture ABI and must not assume the bootloader's temporary machine state is the kernel's permanent state.

## 3. Boot phases

1. Firmware initializes the platform and selects the bootloader.
2. Limine loads the kernel ELF image and requested boot information.
3. Architecture-specific entry receives control using the Limine protocol ABI.
4. Entry code preserves the protocol response pointer and transfers to the Rust bootstrap path.
5. Bootstrap validates all response pointers, lengths, addresses, and memory-map entries.
6. Bootstrap normalizes protocol-specific data into WICOFE boot structures.
7. Kernel reserves its image, boot information, modules, and other non-reclaimable ranges.
8. Early memory initialization establishes the first allocator and required mappings.
9. Architecture HAL establishes kernel-owned CPU state.
10. Kernel proceeds through PMM, VMM, interrupt, timer, heap, and later subsystem initialization.

The order of memory and CPU setup must be refined against the selected Limine protocol revision and each architecture's required entry state.

## 4. Architecture entry separation

The initial design uses separate architecture entry implementations with a shared normalized Rust bootstrap interface.

Suggested source organization:

```text
kernel/src/
├── boot/
│   ├── mod.rs
│   └── info.rs
├── arch/
│   ├── mod.rs
│   ├── x86_64/
│   │   └── mod.rs
│   └── aarch64/
│       └── mod.rs
└── hal/
    └── mod.rs
```

Architecture entry assembly and linker scripts will be introduced in a later implementation step. Common initialization must not contain inline assumptions about x86-64 registers, AArch64 exception levels, or page-table encodings.

## 5. Normalized boot information

General kernel code must consume WICOFE-owned normalized structures, not Limine response structures directly.

The normalized data model must eventually include:
- Memory regions with physical base, length, kind, and relevant attributes.
- Kernel image physical and virtual ranges.
- Higher-half direct-map offset, when provided and validated.
- Boot modules with physical ranges and metadata.
- Framebuffer information when requested and available.
- Firmware/bootloader information only where needed by later kernel services.

Protocol response pointers refer to bootloader-managed memory. The kernel must validate every pointer and range before dereferencing it. Bootloader-reclaimable data must be copied or fully consumed before its memory is released to PMM. No pointer into reclaimable bootloader memory may remain live after reclamation.

Physical addresses and virtual addresses must use distinct types or otherwise be kept unambiguously separate. HHDM addresses are virtual addresses; the offset must not be added twice.

## 6. Memory map and reservations

The memory-map parser must:
- Reject arithmetic overflow and malformed ranges.
- Treat unknown or unavailable regions conservatively as reserved.
- Never allocate firmware, device/MMIO, kernel, module, or boot-information memory as ordinary RAM.
- Reserve the kernel image and all active boot modules before initializing the main PMM.
- Reclaim bootloader-reclaimable ranges only after all references are gone.
- Preserve region boundaries and attributes needed for later zone and DMA policy.

The boot memory map is input to the WICOFE Custom Hybrid Memory Manager. It does not itself establish allocator ownership.

## 7. x86-64 entry obligations

The x86-64 entry implementation must:
- Follow the Limine x86-64 calling convention.
- Establish a known-good stack before calling Rust code.
- Preserve required boot handoff values across assembly setup.
- Avoid assuming the bootloader's GDT remains valid indefinitely.
- Install WICOFE-owned GDT, IDT, and TSS as their respective subsystems become ready.
- Validate CPU features before enabling optional instructions or control-register features.
- Establish kernel-owned page-table and TLB policy through the VMM/HAL.

The exact GDT layout, stack size, linker layout, relocation model, and minimum CPU feature baseline remain to be specified.

## 8. AArch64 entry obligations

The AArch64 entry implementation must:
- Follow the Limine AAPCS64 handoff convention.
- Establish a known-good stack and preserve boot handoff values.
- Detect the current exception level and normalize execution to the selected kernel EL policy.
- Install WICOFE-owned exception vectors (VBAR) and exception-level state.
- Configure translation control, page tables, cache/TLB operations, and interrupt state through the architecture HAL.
- Not assume FP/SIMD/SVE is enabled at entry.
- Validate required CPU features before using them.

The initial EL policy, platform firmware requirements, GIC version baseline, and page-table layout remain open pending target-platform selection.

## 9. ELF and linker policy

ELF is the selected initial kernel executable format because the Limine protocol recommends it and it provides explicit loadable segments and permissions.

The kernel image must use linker scripts and segment permissions that avoid writable-executable segments by default. Relocation and KASLR policy are not yet finalized. Initial implementation may use a fixed, documented link layout, but must not silently assume a load address that conflicts with the boot protocol.

Separate linker scripts are expected for x86-64 and AArch64.

## 10. Boot modules

Boot modules may carry early drivers or other resources in future stages. Each module must have validated bounds, a defined identity/metadata format, and an explicit lifetime. Module bytes must remain reserved until consumed or copied. Boot modules are not automatically trusted merely because the bootloader loaded them.

## 11. Error handling

Invalid or missing required boot responses, malformed memory maps, unsupported architecture state, invalid image ranges, or failed early mappings must stop boot with a minimal diagnostic path. The kernel must not continue with partially validated memory information.

## 12. Testing and acceptance

Boot foundation acceptance requires:
- Host-side unit tests for normalized boot structures and range validation.
- Protocol-structure size/alignment checks for any vendored or authored ABI declarations.
- x86-64 build and boot test under a documented emulator/firmware configuration.
- AArch64 build and boot test under a documented emulator/firmware configuration.
- Validation that reserved and reclaimable memory are handled correctly.
- Evidence that the kernel establishes its own required CPU state before relying on it.

A successful host `cargo check` is not evidence of a bootable kernel.

## 13. Deferred decisions

- Exact Limine release/protocol revision to pin.
- Boot media layout and reproducible image tooling.
- Minimum x86-64 CPU feature level.
- AArch64 platform profile and exception-level policy.
- Kernel virtual address layout and relocation/KASLR.
- Linker script details and stack sizing.
- Boot-module metadata and trust policy.
- Framebuffer and firmware table request policy.
- Emulator matrix and hardware smoke-test targets.

**End of boot foundation specification.**
