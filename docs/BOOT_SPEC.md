# WICOFE Boot Foundation Specification

- **Document:** BOOT_SPEC.md
- **Version:** 1.8
- **Status:** x86-64 BIOS Limine boot and PMM runtime smoke test validated in QEMU
- **Targets:** x86-64 and AArch64
- **Bootloader:** Limine
- **Kernel language:** Rust `no_std`

## 1. Purpose

This document defines the firmware-to-kernel boot contract and the boundary between the bootloader protocol and architecture-neutral WICOFE initialization. The x86-64 BIOS/Limine path has a linked ELF image and passes a QEMU runtime smoke test through PMM frame allocation and release. AArch64 remains compile-checked; its final image and runtime path are not yet validated.

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
│   ├── info.rs
│   ├── validate.rs
│   └── limine.rs
├── arch/
│   ├── mod.rs
│   ├── x86_64/
│   │   └── mod.rs
│   └── aarch64/
│       └── mod.rs
└── hal/
    └── mod.rs
```

Architecture entry trampolines and linker layouts are integrated into the x86-64 final ELF build. The x86-64 BIOS/Limine image is exercised in QEMU, including normalized boot-information consumption and PMM activation. AArch64 entry code is compile-checked but is not yet boot-tested. Common initialization must not contain inline assumptions about x86-64 registers, AArch64 exception levels, or page-table encodings.

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

### 5.1 Limine memory-map response adapter

The Limine memory-map response contains a revision, an entry count, and a pointer to an array of pointers to memory-map entries. Each entry contains a physical base, length, and numeric type. WICOFE mirrors only these response structures in `kernel/src/boot/limine.rs`; the rest of the kernel uses `MemoryRegion`.

The current adapter:
- Copies response entries into caller-provided storage; it does not allocate.
- Checks null pointers, pointer alignment, count conversion, output capacity, zero-length ranges, arithmetic overflow, and overlapping ranges.
- Maps known Limine types into WICOFE memory kinds.
- Preserves unknown future types as `Unknown`, which must never be returned as allocatable RAM.
- Does not claim that a non-null pointer is mapped or readable. The unsafe caller must establish pointer validity and lifetime from the boot handoff contract.

Type mapping:

| Limine type | WICOFE kind |
|---|---|
| Usable | `Usable` |
| Reserved | `FirmwareReserved` |
| ACPI reclaimable | `FirmwareReclaimable` |
| ACPI NVS | `FirmwareReserved` |
| Bad memory | `Bad` |
| Bootloader reclaimable | `BootloaderReclaimable` |
| Executable and modules | `KernelAndModules` |
| Framebuffer | `Device` |
| Reserved mapped | `FirmwareReserved` |
| Unknown future value | `Unknown` |

### 5.2 Limine HHDM response adapter

The HHDM response contains a revision and a 64-bit `offset`, defined by Limine as the virtual address offset of the beginning of the higher-half direct map. WICOFE mirrors this ABI in `LimineHhdmResponse` and normalizes it to the distinct `HhdmOffset` type.

The adapter checks response nullness and alignment before reading it. The unsafe caller remains responsible for proving that the response memory is mapped, readable, and live. The offset is not a physical address, is not itself proof that every physical range is mapped, and must not be added twice. The implementation deliberately does not hard-code a single HHDM base because Limine selects it and it may vary between boots.

### 5.3 Limine executable address response adapter

The Executable Address response contains `physical_base` and `virtual_base`. WICOFE normalizes these into `PhysAddr` and `VirtAddr` inside `ExecutableAddress`, preserving the distinction between physical and virtual address domains.

This response does **not** contain the executable image length. Therefore the adapter does not construct a complete `KernelImage` or reserve a kernel range by itself. The image length must come from trusted linker symbols or validated executable-format metadata before constructing the normalized `KernelImage`. The response pointer is subject to the same unsafe readability, mapping, alignment, and lifetime contract.

### 5.4 Limine module response adapter

The module response contains a module count and a pointer to an array of pointers to `LimineFile` structures. The file structure includes a data address, byte size, path/string metadata, and media information. The adapter mirrors the ABI with `LimineModuleResponse`, `LimineFile`, and `LimineUuid`.

Limine's non-null pointers are HHDM virtual pointers with the HHDM offset already applied. In particular, `LimineFile.address` must not be stored directly as a physical address. `normalize_module_response` subtracts the supplied `HhdmOffset` and emits caller-owned `BootModule` records containing physical base and length.

The adapter:
- Uses caller-provided output storage and performs no allocation.
- Validates response, array, and file-structure pointer nullness/alignment, count conversion, and output capacity.
- Rejects null module data addresses, non-4-KiB-aligned module addresses, addresses below the supplied HHDM offset, zero-sized modules, physical-range overflow, and overlapping module ranges.
- Does not dereference or retain `path` or `string`. If module identity/command metadata is needed, it must be copied into kernel-owned storage before bootloader-reclaimable memory is released.
- Does not prove arbitrary pointers are mapped/readable; the unsafe caller must establish this from the boot handoff contract.

A missing module response is optional according to the protocol. The caller may interpret `NullResponse` as no modules when modules are not required; it must not silently do so when a boot policy requires a particular module. A zero module count is accepted with a null module-array pointer.

These adapters are response decoders, not a complete boot integration. Limine request registration, linker retention, kernel image-size discovery, metadata copying, and architecture entry assembly remain separate work. The official Limine protocol specification is authoritative for ABI field order and pointer semantics.

## 6. Memory map and reservations

The memory-map parser must:
- Reject arithmetic overflow and malformed ranges.
- Treat unknown or unavailable regions conservatively as reserved.
- Never allocate firmware, device/MMIO, kernel, module, or boot-information memory as ordinary RAM.
- Reserve the kernel image and all active boot modules before initializing the main PMM.
- Reclaim bootloader-reclaimable ranges only after all references are gone.
- Preserve region boundaries and attributes needed for later zone and DMA policy.

The boot memory map is input to the WICOFE Custom Hybrid Memory Manager. It does not itself establish allocator ownership.


### 6.1 Early reservation validation

Before PMM initialization, `BootInfo::validate` now enforces the physical
ownership boundary for the kernel image and boot modules:

- The complete kernel image range must be covered by memory-map entries of
  kind `KernelAndModules`, which corresponds to Limine's
  Executable and Modules memory type.
- Every module range must be covered by the same kind. A range outside the
  map, in `Usable` RAM, or in any firmware/device/reserved class is rejected.
- Adjacent `KernelAndModules` entries may jointly cover one image/module
  range, but a gap or a different memory kind may not.
- Kernel and module reservations must not overlap one another. Module
  reservations must also be pairwise disjoint.
- Range-end arithmetic is checked before containment is evaluated. The
  overlap comparisons are only performed after the corresponding ranges have
  passed overflow validation.

This validation feeds the PMM reservation pipeline: the original memory-map
entries are not split or rewritten, but the early usable-memory planner
subtracts the explicit kernel/module/framebuffer ledger before PMM metadata is
placed. Boot information and other bootloader-owned storage remain unavailable
under their existing memory-map ownership class. Bootloader-reclaimable memory
is not released by the current bootstrap and remains unavailable until all
protocol response structures and metadata have been copied or consumed and no
references remain.


### 6.2 Explicit early reservation ledger

`kernel/src/boot/reservations.rs` builds a bounded reservation ledger from
validated `BootInfo` without allocation. It records the kernel image, each
boot module, and the framebuffer when present. Framebuffer physical bytes must
be covered by a `Device` memory-map region; the framebuffer may not overlap
the kernel or a module.

The PMM-facing range predicate accepts a range only when it is completely
covered by `Usable` memory and does not intersect any explicit reservation.
This predicate is a conservative eligibility check, not a frame allocator.
Every non-`Usable` memory kind remains unavailable independently. In
particular, bootloader-reclaimable and firmware-reclaimable memory are not
released by this stage.

The ledger uses caller-provided capacity and reports exhaustion. It does not
split or mutate the Limine memory map, reserve the backing pages of response
structures separately, or initialize PMM. Boot protocol response storage and
the static bootstrap buffers remain protected by their current backing
memory ownership until physical extents and lifetimes are tracked explicitly.

## 7. x86-64 entry obligations

The current x86-64 `_start` trampoline must, and does:
- Follow the Limine x86-64 calling convention.
- Establish a known-good stack before calling Rust code.
- Preserve required boot handoff values across assembly setup.
- Avoid assuming the bootloader's GDT remains valid indefinitely.
- Install WICOFE-owned GDT, IDT, and TSS as their respective subsystems become ready.
- Validate CPU features before enabling optional instructions or control-register features.
- Establish kernel-owned page-table and TLB policy through the VMM/HAL.

The trampoline does not yet install a GDT, IDT, or TSS. The exact descriptor layout, relocation model, and minimum CPU feature baseline remain to be specified. The current Limine request asks for a 64 KiB bootstrap stack.

## 8. AArch64 entry obligations

The current AArch64 `_start` trampoline must, and does:
- Follow the Limine AAPCS64 handoff convention.
- Establish a known-good stack and preserve boot handoff values.
- Detect the current exception level and normalize execution to the selected kernel EL policy.
- Install WICOFE-owned exception vectors (VBAR) and exception-level state.
- Configure translation control, page tables, cache/TLB operations, and interrupt state through the architecture HAL.
- Not assume FP/SIMD/SVE is enabled at entry.
- Validate required CPU features before using them.

The Limine base-revision-6 handoff enters AArch64 at EL1, or at EL2 with VHE when the bootloader is at EL2 and VHE is supported. The current trampoline is EL-neutral and does not change exception level or page tables. The kernel must still select and document its long-term EL policy. Platform firmware requirements, GIC version baseline, and page-table layout remain open pending target-platform selection.

## 9. ELF and linker policy

ELF is the selected initial kernel executable format because the Limine protocol recommends it and it provides explicit loadable segments and permissions.

The kernel image must use linker scripts and segment permissions that avoid writable-executable segments by default. Relocation and KASLR policy are not yet finalized. Initial implementation may use a fixed, documented link layout, but must not silently assume a load address that conflicts with the boot protocol.

Separate linker scripts are expected for x86-64 and AArch64.

### 9.1 Kernel image extent discovery

Architecture-specific linker scripts now define the kernel image boundaries:

- `linker/x86_64.ld`: x86-64 ELF64 higher-half layout.
- `linker/aarch64.ld`: AArch64 ELF64 higher-half layout.
- `__kernel_start`: first byte of the linked image.
- `__kernel_end`: exclusive end of the image, page-aligned after BSS.

Both scripts divide loadable segments into RX text, read-only data, and RW data/BSS program headers. Limine request-section input ranges are retained in the writable data segment because the bootloader patches response pointers and the base-revision support marker. Load memory addresses are image-relative; runtime physical and virtual bases are supplied by Limine's executable-address response.

The entry layer must read the linker symbol addresses and call
`KernelImage::from_linker_bounds` with the physical base corresponding to
`__kernel_start`. The constructor derives the extent and rejects empty or
reversed virtual bounds and physical-range overflow.

The x86-64 linker script is used by the freestanding Cargo binary and its ELF
program headers are checked in CI. Limine loads that image at the expected
higher-half virtual address; the runtime bootstrap reads the linker bounds to
construct the kernel physical reservation. The AArch64 linker script remains
scaffolded and its final link/runtime integration is pending. The x86-64 ELF
layout is validated for the current pinned Limine/QEMU boot configuration,
but should not yet be treated as a stable cross-version boot ABI.


### 9.2 Limine request registration

The request records are defined in `kernel/src/boot/requests.rs` and exported
through `kernel/src/boot/mod.rs`. They request memory map, HHDM, executable
address, modules, framebuffer, and a 64 KiB bootstrap stack. The base-revision
tag requests revision 6. Its third word must be zero after handoff before the
kernel relies on revision-6 guarantees.

The request response fields are interior-mutable because Limine patches them
before entry. The linker scripts retain request records in the writable data
segment. The request delimiters are retained around the request records.

### 9.3 Architecture entry trampolines and handoff gate

The architecture backends now contain minimal Limine entry trampolines:

- x86-64 `_start`: disables interrupts, clears the direction flag, aligns
  the bootloader-provided stack for the System V ABI, and calls
  `wicofe_bootstrap`.
- AArch64 `_start`: masks all interrupt classes and calls
  `wicofe_bootstrap` using the Limine-provided stack and AAPCS64.

The shared Rust entry checks the base-revision marker and passes the required
memory-map, HHDM, and executable-address responses to the normalized boot
information consumer. A missing module response is treated as an empty module
list; framebuffer decoding remains deferred.

The entry obtains `__kernel_start` and `__kernel_end` from the linker and uses
fixed-capacity early buffers (256 memory-map entries and 64 modules). It
validates the response data, constructs kernel/module reservations, plans the
usable physical ranges and PMM metadata placement, then initializes the PMM
through the Limine HHDM. COM1 diagnostics report bootstrap entry, validated
boot information, PMM readiness, and PMM allocation/free smoke-test status.
The static buffers are single-entry bootstrap storage, not a general
allocator. A malformed or unsupported handoff emits a diagnostic and
fail-stops. The entry does not yet install an IDT/VBAR, replace the inherited
page tables, copy module metadata, reclaim bootloader memory, or enable
concurrent PMM use. The AArch64 trampoline is compile-checked but not runtime
validated.

The x86-64 freestanding binary is linked as an ELF executable, checked for
RX/R/RW load segments, packaged into a BIOS Limine ISO, and booted in QEMU by
CI. The test requires the PMM allocation/free smoke-test marker. This is an
initial boot milestone, not a complete OS boot: VMM, exception handling, SMP,
heap, userspace, and filesystem initialization remain pending.


### 9.4 End-to-end normalized boot information consumption

The new `kernel/src/boot/consume.rs` module exposes
`consume_limine_boot_info`, an allocation-free orchestration boundary. It
validates the base-revision and mandatory response-presence gate before
calling the existing memory-map, HHDM, and executable-address adapters. It
then validates the memory map, derives the kernel image extent from the
linker-provided virtual bounds, and verifies that the executable-address
response virtual base matches the linker start symbol.

The module response is optional: a null response produces an empty module
slice; a present response is normalized and checked using the HHDM offset.
All output records are written into caller-provided fixed-capacity slices.
The returned `BootInfo` borrows those slices, so their storage must remain
alive and unmodified while the value is in use.

This stage deliberately does not decode framebuffer details, copy module
path/string metadata, install new mappings, or reclaim bootloader memory. The
bootstrap entry calls this function using linker bounds and static
fixed-capacity storage, then constructs reservations and activates PMM. The
unsafe caller must prove that response structures and nested pointers are
mapped, readable, aligned, and live; pointer nullness/alignment checks alone
cannot establish memory safety. The complete x86-64 path is linked and
validated by the BIOS/Limine QEMU test. VMM and AArch64 runtime validation
remain outstanding.

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

- Exact Limine bootloader release to pin (the request records currently target protocol base revision 6).
- Boot media layout and reproducible image tooling.
- Minimum x86-64 CPU feature level.
- AArch64 platform profile and exception-level policy.
- Kernel virtual address layout and relocation/KASLR.
- Linker script details and stack sizing.
- Boot-module metadata and trust policy.
- Framebuffer and firmware table request policy.
- Emulator matrix and hardware smoke-test targets.

**End of boot foundation specification.**
