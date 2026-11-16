# WICOFE Memory Management Specification

- **Document:** MEMORY_MANAGEMENT_SPEC.md
- **Version:** 1.3
- **Status:** Design baseline (implementation parameters remain open)
- **Applies to:** x86-64 and AArch64
- **License:** MPL-2.0 for original WICOFE documentation

## 1. Purpose and scope

This document establishes the memory-management architecture for the WICOFE kernel. It records the selected Custom Hybrid Memory Manager approach. It is a design specification; it does not claim that any allocator, page-table manager, or memory subsystem has been implemented.

The subsystem is divided into four cooperating areas:

1. Physical Memory Manager (PMM): ownership and allocation of physical frames.
2. Virtual Memory Manager (VMM): virtual address spaces, mappings, page tables, and protection.
3. Kernel heap: dynamic allocation for kernel objects and variable-sized data.
4. Memory policy and accounting: zones, constraints, reservations, and resource tracking.

The design must support x86-64 and AArch64 without exposing architecture-specific page-table details to common allocator policy.

## 2. Selected architecture: Custom Hybrid Memory Manager

WICOFE will use a project-designed hybrid architecture combining established allocation techniques:

| Layer | Selected mechanism | Primary role |
|---|---|---|
| Early boot allocation | Bitmap-based frame tracking / bounded early allocator | Bootstrap memory before the full PMM is online |
| Main physical allocator | WICOFE-managed buddy allocator | Allocate and release contiguous physical frame blocks |
| PMM metadata | Bitmap plus per-region/per-frame metadata | Track frame state, reservations, and diagnostics |
| Virtual memory | Architecture-specific page-table backends behind common VMM interfaces | Manage mappings and address spaces |
| Small kernel objects | Slab or size-class allocator | Reduce overhead for frequent small allocations |
| Large kernel allocations | Page-backed allocations through PMM/VMM | Serve larger and page-aligned requests |

This is a custom system architecture, not a commitment to invent every underlying algorithm from scratch. The buddy allocator is the primary physical-frame allocation algorithm; bitmap tracking supports bootstrapping, state tracking, and validation.

## 3. Design principles

- Correctness and memory safety take priority over micro-optimizations.
- Physical and virtual addresses must be represented as distinct types.
- Every allocation must have explicit ownership, lifetime, alignment, and release rules.
- Reserved, unavailable, and device memory must never be returned as ordinary usable RAM.
- Allocation failure must be explicit; silent success with an invalid or partial allocation is forbidden.
- Architecture-specific operations belong behind HAL/VMM interfaces.
- Allocator metadata must be protected against concurrent access on SMP systems.
- Interrupt-context allocation rules must be explicit; potentially blocking allocation must not be used from hard interrupt context.
- Memory permissions must follow least privilege, including writable/executable separation where supported.
- Unsafe Rust blocks must document the invariants they rely on.

## 4. Physical Memory Manager (PMM)

### 4.1 Memory map ingestion

At boot, the kernel receives a platform/bootloader memory map. The early memory code must validate ranges, arithmetic overflow, alignment, overlaps, and address-width constraints before using the data.

Memory regions are classified at minimum as:

- Usable RAM.
- Firmware/bootloader-reserved memory.
- Kernel image and loaded-module memory.
- Memory-map and boot-information storage.
- Device/MMIO regions.
- ACPI or platform-reclaimable memory where applicable.
- Bad, unknown, or explicitly unavailable memory.

Unknown or malformed regions are treated conservatively as unavailable. Reclaimable regions become usable only after the kernel has completed the relevant handoff and explicitly releases them.


### 4.1.1 Explicit early boot reservations

The boot boundary materializes a bounded reservation list in caller-owned
storage before PMM initialization. The initial reservation kinds are:

- `KernelImage`: the complete physical kernel image extent.
- `BootModule`: each active boot-loaded module extent.
- `Framebuffer`: the framebuffer physical byte range, when decoded.

The list is an exclusion ledger, not an allocator and not a rewritten memory
map. The PMM may consider a candidate range allocatable only when it is fully
covered by `Usable` memory and does not intersect any reservation. Unknown,
reserved, device, firmware-reclaimable, bootloader-reclaimable, and
kernel/module memory-map classes remain unavailable regardless of the
reservation list.

The builder validates the complete `BootInfo` before writing and reports
insufficient caller capacity explicitly. Framebuffer ranges must be covered
by `Device` memory-map entries. Reservations must be non-empty, non-overflowing,
and mutually disjoint. This early list does not yet account for every
bootloader response structure or static bootstrap buffer: those remain
protected by their enclosing memory-map ownership class (or kernel image)
until their physical backing ranges can be identified and explicitly tracked.

### 4.1.2 Early usable-memory plan

The boot memory foundation derives a bounded `EarlyMemoryPlan` from the
validated memory map and explicit reservation ledger before PMM metadata is
placed. The planner:

- considers only `Usable` memory-map descriptors;
- validates the map, including unsorted input, overflow, and overlap;
- subtracts every explicit reservation from each usable interval;
- aligns candidate interval boundaries inward to the selected power-of-two
  page size;
- returns ranges sorted by physical base while preserving adjacent source
  descriptors as separate entries;
- excludes bootloader-reclaimable, firmware-reclaimable, device, reserved,
  unknown, and all other non-usable classes;
- uses caller-owned output storage and performs a counting pass before
  writing, so insufficient capacity leaves the output unchanged.

The planner rejects empty, overflowing, or overlapping reservations. Its
output is only a candidate-range plan: it does not allocate memory, reserve
metadata storage, initialize bitmap state, mutate the boot memory map, or
transfer ownership. The caller must reserve the physical backing of PMM
metadata before any corresponding frames can be exposed to the allocator.
Reclaimable memory remains excluded until its metadata and all references
have been safely consumed and its lifetime has ended.

### 4.1.3 PMM metadata capacity and placement

The metadata-placement planner consumes the sorted, page-aligned output of
the early usable-memory planner. It calculates the tracked frame count across
all candidate ranges, sizes a one-bit-per-frame bitmap, and sizes a
caller-selected fixed-width per-frame metadata array. All arithmetic is
checked and the combined byte count is rounded up to the selected page size.

The initial placement policy is deterministic first-fit: reserve the required
contiguous metadata extent at the beginning of the first candidate range large
enough to contain it. The planner returns the physical base, page-rounded
reserved length, tracked frame count, bitmap byte count, and per-frame
metadata byte count. It also returns a new candidate-range list with the
metadata backing extent removed. This prevents the PMM from later allocating
the frames that contain its own metadata.

Input ranges must be sorted, page-aligned, non-empty, and non-overlapping;
adjacent ranges remain distinct. Metadata backing must fit wholly inside one
candidate range; this initial planner does not coalesce adjacent descriptors
to satisfy a contiguous metadata request. Caller-owned output capacity is
checked before writes. The planner does not initialize the bitmap or frame
records, map the backing range, establish synchronization, or transfer memory
ownership. The caller must complete those operations before PMM activation.
The per-frame record size and eventual record encoding remain implementation
parameters rather than a stable ABI.

### 4.1.4 Initial bitmap frame allocator

The first executable PMM implementation is a bounded, single-frame bitmap
allocator over the discontiguous ranges produced by the early-memory pipeline.
It receives caller-owned bitmap and per-frame metadata slices; initialization
checks page size, range ordering, alignment, overlap, arithmetic, and storage
capacity before modifying either slice.

The bitmap is the authoritative allocation state (one bit per frame). A
16-byte `FrameMetadata` record stores a caller-defined owner tag, diagnostic
flags, and allocation identifier. The allocator supports single-frame
allocation, release, free/allocated counters, and metadata inspection. It
rejects unaligned and untracked addresses, double-free, and exhaustion. The
allocation scan uses a next-fit hint and naturally traverses multiple physical
ranges without treating gaps as RAM.

The initial implementation is explicitly single-core. Callers must serialize
all access; it must not be used concurrently from multiple CPUs or interrupt
contexts until synchronization and execution-context rules are implemented.
The caller must ensure the supplied ranges already exclude the kernel image,
boot modules, framebuffer, PMM metadata backing, and every other reservation.
The allocator does not map physical metadata storage, reclaim bootloader memory,
initialize page tables, or provide contiguous multi-frame allocations.

This bitmap allocator is the correctness baseline for frame ownership. The
buddy allocator, zone constraints, SMP synchronization, and integration with
the VMM remain subsequent milestones.

### 4.2 Early allocator

A bounded early allocator is used before the full PMM and kernel heap are ready. It may use bitmap frame tracking and monotonic allocation for a limited bootstrap phase.

Requirements:

- It must allocate only from validated usable RAM.
- It must honor alignment and reserved ranges.
- Its allocations must be recorded so they can be adopted by the full PMM or permanently reserved.
- It must have a defined exhaustion error.
- It must be disabled or isolated after PMM initialization to prevent two allocators from owning the same frames.

### 4.3 Buddy allocator

The main PMM uses a buddy allocator to manage contiguous blocks of physical frames.

The initial design uses power-of-two block orders. Allocation splits larger free blocks as needed; freeing attempts to coalesce a block with its free buddy when both are in the same eligible region and order.

The implementation must define and test:

- Minimum allocation order and maximum supported order.
- Frame size and alignment assumptions.
- Free-list organization and synchronization.
- Split and coalesce invariants.
- Handling of memory-region boundaries and holes.
- Allocation failure behavior.
- Metadata consistency checks.
- Protection against double-free, invalid-free, and freeing reserved frames.

The exact order limits, metadata representation, and locking strategy remain implementation decisions to be benchmarked.

### 4.4 Bitmap and frame metadata

Bitmap state and per-frame metadata complement the buddy free lists. They support region initialization, reservation tracking, diagnostics, and consistency checking. They are not an independent source of allocation ownership that may conflict with the buddy allocator.

The implementation must define a single authoritative state-transition protocol so bitmap/metadata and buddy free lists cannot diverge. Metadata storage itself must be reserved and accounted for before the corresponding memory is made allocatable.

### 4.5 Memory zones and constraints

The PMM must support zone-aware allocation. Initial zone boundaries and policies are platform-profile decisions, not assumed to be identical across architectures.

The interface should be able to express constraints such as:

- Maximum physical address.
- Required alignment.
- Contiguous frame count or allocation order.
- DMA/device reachability limits.
- Exclusion of particular memory regions.
- Zeroed memory requirement.

DMA-capable memory must be allocated according to the device's addressability and coherency requirements. An IOMMU, cache maintenance, and DMA mapping policy are separate concerns and must not be inferred merely from physical contiguity.

NUMA-aware placement may be added later; it is not required for the first bootable milestone.

## 5. Virtual Memory Manager (VMM)

The VMM manages virtual address spaces and page-table operations. Common kernel code must use architecture-neutral interfaces while x86-64 and AArch64 backends implement their respective translation-table formats and hardware operations.

Responsibilities include:

- Kernel address-space initialization.
- User address-space creation and destruction.
- Map, unmap, and permission changes.
- Page-fault integration.
- Guard regions and unmapped-region policy.
- TLB invalidation and SMP shootdown coordination.
- Mapping ownership and lifetime tracking.
- Architecture-specific page-size and translation-feature discovery.

The VMM must distinguish physical frames from virtual addresses and must validate canonical/architecturally valid address ranges, alignment, permissions, and mapping conflicts.

Page-table memory is allocated through PMM under explicit constraints. Page-table updates must be synchronized, and stale TLB translations must not outlive permission or ownership changes.

The exact virtual address-space layout, supported page sizes, huge-page policy, kernel/user split, and architecture feature requirements remain open decisions.

## 6. Kernel heap

The kernel heap provides dynamic allocation for objects that are not naturally represented as page allocations.

The selected direction is a slab or size-class allocator for common small object sizes, backed by page allocations from PMM/VMM. Large or specially aligned allocations may bypass small-object caches and use page-backed allocation paths.

Requirements:

- Explicit alignment guarantees.
- Defined allocation and deallocation contracts.
- No allocator recursion during heap bootstrap.
- Per-cache object-size and alignment validation.
- Safe cache teardown rules if caches become dynamic.
- SMP-safe allocation and deallocation.
- Optional per-CPU caches only after correctness and locking rules are established.
- Defined behavior for allocation from interrupt context.

The initial cache sizes and allocator implementation are not frozen by this specification.

## 7. Allocation interface and flags

Common kernel callers should request memory through typed interfaces rather than directly manipulating allocator internals.

The eventual API should express intent and constraints such as:

- Zeroed versus uninitialized memory.
- Physical contiguity requirements.
- Maximum physical address.
- Alignment.
- DMA suitability.
- Blocking versus non-blocking context.
- Reclaimability or lifetime class where supported.

Flags must not promise properties the platform cannot provide. Unsupported combinations must return a documented error.

Rust APIs should prefer ownership-bearing handles or types that make release responsibility clear. Raw physical addresses and raw virtual pointers must not be interchangeable.

## 8. Concurrency, interrupts, and SMP

PMM, VMM, and heap synchronization must be designed for multiple CPUs.

The implementation must document:

- Lock ordering between PMM, VMM, and heap layers.
- Whether each operation may block.
- Which operations are permitted in interrupt context.
- Per-CPU state and initialization requirements.
- TLB shootdown coordination and timeout/failure handling.
- Reentrancy constraints for allocator metadata operations.

No allocator lock may be held across an operation that can recursively allocate from the same allocator unless a proven recursion-safe design is used.

## 9. Initialization order

The expected dependency sequence is:

1. Validate boot handoff and memory map.
2. Establish minimal architecture mappings needed to execute safely.
3. Reserve kernel, boot data, modules, and allocator metadata.
4. Initialize early frame tracking and bounded early allocations.
5. Initialize the main PMM buddy allocator and validate its metadata.
6. Initialize the VMM backend and required kernel mappings.
7. Enable required TLB and address-space operations.
8. Initialize the kernel heap backed by PMM/VMM.
9. Enable general subsystem allocations.
10. Reclaim eligible boot/firmware memory only after its final use.

The exact sequence is platform- and boot-protocol-dependent. No stage may expose memory to general allocation before ownership and mapping invariants are established.

## 10. Error handling and observability

The subsystem must expose explicit errors for at least:

- Out of memory.
- Invalid alignment or size.
- Address-range overflow.
- Unsupported constraint.
- Invalid or duplicate free.
- Reserved/unavailable frame.
- Mapping conflict.
- Invalid address or permission.
- Resource exhaustion during page-table creation.

Debug builds should provide allocator consistency checks, reservation diagnostics, allocation counters, and optional leak/ownership tracing. Diagnostic facilities must not be required for correctness.

## 11. Security and reliability

- Validate all boot-provided memory descriptors.
- Prevent use-after-free and double-free through ownership rules and debug validation.
- Clear memory before exposing it to less-privileged domains when required by the security policy.
- Enforce W^X where supported and avoid writable-executable mappings by default.
- Do not map MMIO as ordinary cacheable RAM.
- Validate user-supplied virtual ranges before mapping or copying.
- Ensure page-table teardown and TLB invalidation complete before reusing frames.
- Treat allocator metadata corruption as a kernel integrity failure.

## 12. Testing and acceptance criteria

Before the memory subsystem is considered complete, it requires:

- Unit tests for buddy split/coalesce and bitmap state transitions.
- Property-based or randomized allocation/free sequences.
- Tests for holes, reserved ranges, region boundaries, and overflow.
- Double-free, invalid-free, and exhaustion tests.
- Alignment and maximum-address constraint tests.
- PMM/VMM integration tests.
- Page-table permission and mapping lifecycle tests.
- SMP stress tests and TLB shootdown validation.
- Architecture-specific tests on x86-64 and AArch64.
- Boot tests with memory maps representative of supported platforms.
- Benchmarks for latency, fragmentation, and scalability before performance claims are made.

A successful compile alone is not acceptance evidence.

## 13. Deferred design decisions

The following remain open until target and implementation constraints are known:

- Physical frame size assumptions and supported page sizes.
- Buddy order limits and free-list structure.
- Bitmap and per-frame metadata encoding.
- Exact zone boundaries and DMA policy.
- NUMA support and placement policy.
- Virtual address-space layout per architecture.
- Huge-page policy.
- Page-table locking and TLB shootdown implementation.
- Slab cache sizes and per-CPU cache policy.
- Reclaim, swap, memory compression, and overcommit policy.
- Public/internal allocator API names and error type.

## 14. Implementation milestones

1. Boot memory-map parser and range validation.
2. Reserved-region model and early allocator.
3. PMM metadata and buddy allocator.
4. PMM tests and invariant checking.
5. Architecture-specific VMM backends.
6. Address-space and mapping lifecycle.
7. TLB invalidation and SMP coordination.
8. Kernel heap with slab/size classes.
9. DMA constraints and platform-specific memory zones.
10. Stress testing, diagnostics, and performance evaluation.

**End of memory management specification.**
