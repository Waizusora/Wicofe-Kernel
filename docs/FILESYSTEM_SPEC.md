# WICOFE Filesystem and VFS Specification

- **Document:** FILESYSTEM_SPEC.md
- **Version:** 0.1
- **Status:** Draft contract specification
- **Scope:** VFS, native ONWFS, built-in FAT32, and future filesystem modules

## 1. Purpose and status

This document establishes the filesystem architecture and requirements for WICOFE. It is a design specification only. It does not define a finalized ONWFS disk format and does not claim that VFS, ONWFS, FAT32, journaling, or recovery code exists.

The selected name for the native filesystem is **ONWFS — Oh Nice WICOFE Filesystem**. FAT32 is planned as an additional built-in filesystem. Other compatible filesystems may be delivered as DKM where boot and dependency constraints permit.

## 2. Architecture overview

The filesystem stack is divided into:

1. **VFS core:** common namespace, mount, file-object, and operation contracts.
2. **Filesystem implementations:** ONWFS, FAT32, and future local or network filesystems.
3. **Block I/O layer:** requests against block devices, independent of filesystem semantics.
4. **Device drivers:** storage controllers and devices, built-in or DKM according to boot requirements.

A filesystem must not directly depend on a particular storage controller. It uses the block I/O contract or an explicitly defined alternative such as a network transport.

## 3. VFS responsibilities

VFS owns:
- Mount table and namespace policy.
- Path traversal and mount-point crossing.
- Filesystem instance lifecycle.
- Inode/vnode-like object cache and references.
- Open file objects and operation dispatch.
- Directory iteration contracts.
- File descriptor integration with the process subsystem.
- Common metadata representation and error translation.
- Capability reporting for filesystem-specific operations.

VFS does not silently invent filesystem capabilities. If a filesystem cannot implement an operation with correct semantics, it must report a defined unsupported-operation error.

## 4. Core object model

The initial logical objects are:

### 4.1 Filesystem instance

Represents one mounted or mountable filesystem instance. It owns implementation-private state and provides a versioned operation table.

### 4.2 Mount

Associates a filesystem root with a namespace mount point. The design must define mount lifetime, mount namespaces, propagation policy, and behavior when a mount is busy.

### 4.3 Inode/vnode

Represents a filesystem object independently of its pathname. It carries a stable identity within the filesystem instance and exposes metadata and operations supported by that object.

### 4.4 Dentry/path component

Caches or represents a name-to-object relationship. A pathname is not an object identity; rename and unlink semantics must be defined independently of cached path strings.

### 4.5 Open file object

Represents an opened instance, including access mode, status flags, current offset where applicable, and a reference to the underlying object. Duplicated descriptors may share an open file object according to Unix-like semantics.

### 4.6 File descriptor

A process-visible handle to an open file object. Descriptor allocation, close-on-exec behavior, duplication, and per-process ownership belong to the process/file-descriptor layer, not to an individual filesystem.

These are logical roles. Exact Rust types, locking, reference counting, and cache policies remain open until the VFS contract is reviewed.

## 5. Required VFS operations

The VFS interface should account for:
- Mount and unmount.
- Lookup and path traversal.
- Create and remove regular files.
- Create and remove directories.
- Open, close, read, write, and seek.
- Readdir/iterate directory.
- Rename.
- Get and set supported metadata.
- Readlink and symlink creation where supported.
- Hard-link creation where supported.
- Truncate and sparse-file operations where supported.
- Synchronization and durability requests.
- Filesystem statistics and capability queries.

The contract must define error codes, partial I/O behavior, offset semantics, concurrency, and object lifetime. Operations must not assume every filesystem supports every feature.

## 6. Unix-like metadata and semantics

The common metadata model should be able to represent:
- Object type.
- File size and allocated size where available.
- UID and GID.
- Permission bits.
- Link count where meaningful.
- Access, modification, status-change, and creation timestamps where available.
- Filesystem and object identifiers.
- Flags and extended attributes where supported.

The VFS must distinguish unavailable metadata from a real zero value. FAT32 does not natively store Unix UID/GID, POSIX mode bits, symlinks, or hard links. Any mount-time emulation policy must be explicit, configurable, and documented as synthetic rather than native on-disk semantics.

## 7. ONWFS — native filesystem

### 7.1 Goals

ONWFS is intended to provide:
- Inode-based object identity.
- Hierarchical directories.
- Symbolic and hard links.
- UID/GID and POSIX-style permission metadata.
- Nanosecond-capable timestamps if supported by the final format and implementation.
- Extended attributes.
- Sparse files.
- Metadata checksums.
- Journaling and crash recovery.
- Allocation policies suitable for SSDs and other block devices.
- Explicit format versioning and feature compatibility.

These are design goals, not guarantees of an already finalized format.

### 7.2 Format design requirements

A separate ONWFS format review must define at least:

- Fixed logical block size policy and supported device sector sizes.
- Primary and backup superblock strategy.
- Magic, format version, compatible/read-only-compatible/incompatible feature flags.
- Endianness and alignment rules.
- Filesystem UUID and generation identifiers.
- Inode layout, inode numbering, and reserved inode policy.
- Directory record format and name encoding/normalization policy.
- Extent or block mapping format.
- Free-space allocation metadata and consistency rules.
- Link-count and orphan/unlinked inode handling.
- Timestamp epoch, resolution, and range.
- Extended-attribute storage and limits.
- Metadata checksum algorithm and checksum coverage.
- Journal layout, transaction format, ordering, and commit record.
- Recovery algorithm and behavior after interrupted writes.
- Backup metadata and repair policy.
- Maximum file, directory, filesystem, and name sizes.
- Clean/dirty state and mount policy.
- Upgrade and downgrade compatibility rules.

No on-disk structure should be treated as stable until this format review is accepted and versioned.

### 7.3 Journaling and durability

The design must distinguish metadata consistency from full data journaling. The chosen mode (for example, metadata journaling or ordered data) must be explicit.

The implementation must define write ordering, flush/barrier behavior, transaction boundaries, replay rules, and handling of torn or corrupt journal records. A successful write syscall must not be described as durable unless the relevant persistence contract is met. Synchronization operations must have defined guarantees.

### 7.4 Checksums and recovery

Checksums should detect corruption in selected metadata and journal structures. The format must specify the algorithm, seed, coverage, and versioning. Checksums do not replace authentication or backups.

Recovery must be idempotent or otherwise prove that repeated recovery attempts cannot corrupt the filesystem further. If recovery cannot safely proceed, the filesystem must fail closed and provide a diagnostic path rather than silently mounting inconsistent metadata read-write.

### 7.5 SSD-aware allocation

Allocation policy may consider sequentiality, locality, discard/TRIM support, write amplification, and device characteristics. The filesystem must not assume all block devices are SSDs or that discard is safe or supported. Device capabilities are queried through the block layer.

## 8. FAT32 support

FAT32 is planned as an additional built-in filesystem for interoperability and removable media.

Requirements:
- Validate boot sector and BPB fields before use.
- Validate cluster size, FAT length, data-region bounds, and cluster chains.
- Detect loops and out-of-range cluster references.
- Handle end-of-chain and bad-cluster markers correctly.
- Enforce bounds on directory traversal and file reads.
- Define behavior for long file names and malformed LFN sequences.
- Avoid writes that would corrupt unsupported FAT variants.
- Clearly report read-only or unsupported features.

FAT32 does not natively provide POSIX symlinks, hard links, Unix ownership, or full POSIX permission semantics. WICOFE must not claim those features are stored natively on FAT32. A conservative initial implementation may be read-only until write-path consistency and power-failure behavior are validated.

## 9. Filesystem modules

Future filesystems may be supplied as DKM. The filesystem module interface must use the versioned C-compatible DKM boundary and a VFS operation table; it must not expose Rust trait-object ABI.

A filesystem module must declare:
- Supported VFS interface version.
- Supported mount options.
- Required block or network interfaces.
- Read/write and feature capabilities.
- Required kernel configuration.
- License and provenance metadata.

Boot-critical filesystem support must be available before root mount. A filesystem module stored only on the filesystem it is needed to mount creates a bootstrapping cycle and is not acceptable without an independent early-load path.

## 10. Block I/O contract

The block layer must expose device capacity, logical/physical sector constraints where known, supported operation flags, and asynchronous or synchronous request completion semantics.

The contract must define:
- Read/write request bounds and alignment.
- Flush and ordering/barrier operations.
- Optional discard/TRIM.
- Error propagation and partial completion.
- Device removal and in-flight request cancellation.
- DMA constraints and buffer ownership.
- Concurrency and queue-depth behavior.

Filesystems must not bypass the block layer to access controller registers or assume a particular storage bus.

## 11. Error and concurrency semantics

VFS and filesystem contracts must define:
- Stable error mapping.
- Partial reads/writes.
- Concurrent lookup, rename, unlink, and open behavior.
- Lock ordering and lock context.
- Reference ownership and release.
- Deadlock avoidance.
- Behavior when a device disappears or reports I/O failure.
- Read-only fallback after detected corruption where safe.

No filesystem operation may retain an unvalidated user pointer. User buffers must be validated and copied or accessed through a documented safe user-memory interface.

## 12. Security requirements

- Path traversal must enforce namespace and permission policy.
- Mount options must not silently weaken security.
- Filesystem parsers must treat disk contents as untrusted input.
- Integer arithmetic, offsets, lengths, and allocation sizes must be checked.
- Symlink traversal limits must prevent loops and unbounded recursion.
- Ownership and mode checks must be centralized through the security/VFS contract.
- Corruption must not lead to unchecked kernel memory access.

## 13. Testing and validation

### VFS
- Path traversal, mount crossing, rename, unlink, open-after-unlink.
- Reference lifetime and concurrent operations.
- Error mapping and unsupported capability behavior.
- Descriptor duplication and shared open-file offsets.

### ONWFS
- Format parser fuzzing.
- Boundary and overflow cases.
- Allocation and free-space consistency.
- Link-count and orphan recovery.
- Journal replay after simulated power loss at every write boundary.
- Metadata checksum detection.
- Read-only mount of unknown compatible features.
- Refusal of unknown incompatible features.
- Corrupt superblock and backup-superblock scenarios.
- Long-running stress and randomized filesystem operations.

### FAT32
- Malformed BPB and directory records.
- Cluster-chain loops and out-of-range references.
- Truncated images and boundary-crossing files.
- LFN corruption and fallback behavior.
- Read/write consistency if write support is enabled.

Testing must use generated disk images and fault injection. A successful mount of one sample image is not sufficient validation.

## 14. Decisions still open

- Exact VFS object types, locking, and cache policy.
- Mount namespace and propagation semantics.
- ONWFS block size and on-disk format.
- ONWFS extent versus alternative block mapping.
- Journal mode and transaction protocol.
- Checksum algorithm.
- Filename encoding and normalization.
- Timestamp epoch and precision.
- Maximum size limits.
- FAT32 initial read-only versus read/write scope.
- Filesystem DKM interface version and boot-time module transport.
- fsck/format utility interface (to be designed with userland).

## 15. Completion criteria

The filesystem subsystem is not complete when it can merely mount an image. Completion requires a reviewed VFS contract, versioned ONWFS format, consistency and recovery implementation, corruption handling, fuzzing, crash-injection tests, FAT32 conformance tests, and validated integration with block I/O and file descriptors.

**End of filesystem specification draft.**
