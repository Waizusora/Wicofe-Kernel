# Wicofe-Kernel Third-Party Licensing

**Policy authority:** Wicofe Dependency License Policy 0.2  
**Original Wicofe code:** MPL-2.0 unless a repository-specific notice states otherwise.

## Limine bootloader 12.9.6

| Field | Evidence |
|---|---|
| Upstream | https://github.com/Limine-Bootloader/Limine |
| Release | v12.9.6 |
| Source commit | `89a9512c623a4779cc73e3869383876d54264b28` |
| Release archive | `limine-binary.tar.xz` |
| SHA-256 | `939f670e0831cc940453fd3a0ddfb2b324c0b4e3c5d0e741c190aa0d13df0103` |
| Upstream license | BSD-2-Clause |
| License evidence | [COPYING](https://github.com/Limine-Bootloader/Limine/blob/89a9512c623a4779cc73e3869383876d54264b28/COPYING) |
| Third-party notices | [3RDPARTY.md](https://github.com/Limine-Bootloader/Limine/blob/89a9512c623a4779cc73e3869383876d54264b28/3RDPARTY.md) |

The v12.9.6 annotated tag object was signature-verified by GitHub's API. The release archive digest published by GitHub matches the SHA-256 pinned in `scripts/build_limine_iso.sh`. The release archive's detached signature has not been independently verified.

The upstream `3RDPARTY.md` documents bundled components under 0BSD, BSD-2-Clause, MIT, Apache-2.0, CC0-1.0 and CC-BY-3.0-derived terms. No GPL/AGPL/LGPL identifier was found in the reviewed `COPYING` and `3RDPARTY.md` texts. This finding is limited to those notices and does not replace a full binary archive SBOM.

## Distribution notices

The exact upstream `COPYING` and `3RDPARTY.md` files are preserved in:

- `third_party/licenses/limine-12.9.6/COPYING`
- `third_party/licenses/limine-12.9.6/3RDPARTY.md`

The ISO build script now requires both files and copies them to:

`/boot/limine/licenses/limine-12.9.6/`

This preserves the BSD-2-Clause notice and the upstream third-party attribution/license notices alongside the Limine binaries. The resulting ISO still requires a build-and-inspect verification after this change; no successful ISO build is claimed here.

## Build and test toolchain findings

The Limine host utility is compiled directly from the root-level C99 source in the pinned binary archive using pinned `clang-22`; GNU Make is no longer invoked. Archive SHA-256 verification and safe `.tar.xz` extraction use Python's standard library (`hashlib` and `tarfile` with the data extraction filter), removing GNU coreutils `sha256sum` and GNU tar from the build script. ELF inspection uses `llvm-readelf-22` rather than GNU binutils `readelf.

The remaining CI dependencies include:

| Tool | Role | License status under Policy 0.2 |
|---|---|---|
| QEMU 10.2.1 | x86-64 boot test | REJECTED — GPL-2.0-or-later; build/test only, not shipped |
| xorriso 1.5.6 | ISO image creation | REJECTED — GPL-2.0-or-later; build-only, not shipped |
| LLVM 22.1.2 tools | Inspect ELF headers and program headers | NEEDS_REVIEW — Apache-2.0 WITH LLVM-exception |
| curl | Download pinned Limine release | NEEDS_REVIEW — upstream curl license; build-only |

QEMU is retained temporarily because it provides the current automated PC/BIOS boot test. Bochs is a candidate x86 emulator under LGPL-2.1, but it requires conditional approval and a parity test; it does not cover AArch64. xorriso is retained temporarily because it reliably creates the current BIOS-bootable ISO. cdrtools/mkisofs (CDDL-1.0) and libisofs/libburn (LGPL-family) are candidates, each requiring license review and a bootable-ISO parity test. These GPL tools remain release blockers under the literal Policy 0.2 rule; they have not been silently exempted because they are not shipped.

## Remaining review

- Review Limine's binary archive contents and reconcile them with `3RDPARTY.md`.
- Verify the detached release archive signature independently.
- Replace or obtain explicit policy resolution for GPL QEMU and xorriso; do not remove them before replacement parity tests.
- Verify the final ISO contains the notices at the stated path and publish its SBOM.
- Security and maintenance status for this Limine release remain NOT_ASSESSED.

Limine is retained because it is a mature bootloader component; replacing it with an in-house bootloader would increase security and maintenance risk. No GPL/AGPL issue was identified in the reviewed upstream notices.
