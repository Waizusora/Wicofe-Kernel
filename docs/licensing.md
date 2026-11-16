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

The exact upstream `COPYING` and `3RDPARTY.md` files are preserved in the repository; the `LICENSE` file from the pinned binary archive is also copied when present. The ISO build verifies the archive's `3RDPARTY.md` against the pinned source copy.

- `third_party/licenses/limine-12.9.6/COPYING`
- `third_party/licenses/limine-12.9.6/3RDPARTY.md`

The ISO build script now requires both files and copies them to:

`/boot/limine/licenses/limine-12.9.6/` (including the archive `LICENSE` when present)

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

## Full CI package-closure evidence — 2026-11-16

The latest Kernel CI run [43880975214](https://github.com/Waizusora/Wicofe-Kernel/actions/runs/43880975214) completed successfully and uploaded [build and package-license evidence artifact 14232042726](https://github.com/Waizusora/Wicofe-Kernel/actions/runs/43880975214/artifacts/14232042726). It used the pinned Wicofe-Tools auditor revision `cccc4c197687ce8fd629f9b9d8a7d95d4e89cddc`.

The same run verified formatting, unit tests, x86-64 target/build, Limine ISO generation, ISO notice presence, QEMU BIOS boot and the AArch64 target check. This demonstrates current workflow functionality only; it does not waive the license policy.

The installed CI package delta contains 103 package records:

| Package inventory status | Count |
|---|---:|
| REJECTED | 15 |
| NEEDS_REVIEW | 84 |
| UNKNOWN | 4 |
| Total | 103 |

The 15 REJECTED package records are:

| Package | Version | Primary license evidence |
|---|---|---|
| `cpu-checker` | `0.7-1.4build1` | GPL-3 |
| `ipxe-qemu` | `1.21.1+git-20250829.969ce2c55+dfsg-3ubuntu2` | GPL-2+ |
| `libburn4t64` | `1.5.6-1.2build1` | GPL-2+ |
| `libcdparanoia0` | `3.10.2+debian-14ubuntu2` | GPL-2+ |
| `libfdt1` | `1.7.2-2ubuntu1` | GPL-2+ |
| `libfftw3-single3` | `3.3.10-2fakesync1build3` | GPL-2+ |
| `libiscsi7` | `1.20.0-4build1` | GPL-2+ |
| `libisoburn1t64` | `1:1.5.6-1.1ubuntu4` | GPL-2+ |
| `libisofs6t64` | `1.5.6.pl01-1.1ubuntu3` | GPL-2+ |
| `libv4l-0t64` | `1.32.0-2ubuntu1` | GPL-2 |
| `libv4lconvert0t64` | `1.32.0-2ubuntu1` | GPL-2 |
| `libvte-2.91-0` | `0.84.0-2` | GPL-3+ AND LGPL-3+ AND Expat |
| `libvte-2.91-common` | `0.84.0-2` | GPL-3+ AND LGPL-3+ AND Expat |
| `msr-tools` | `1.3+git20220805.7d78c80-1ubuntu0.1` | GPL-2 |
| `xorriso` | `1:1.5.6-1.1ubuntu4` | GPL-2+ |

These records come from the installed package delta, not a proven minimal runtime closure. The audit has not yet mapped each package to a direct dependency, transitive dependency, recommended package or preinstalled host utility. The xorriso closure is directly relevant: `libburn4t64`, `libisoburn1t64` and `libisofs6t64` are part of its installed library stack. The remaining records require dependency-edge/usage mapping before deciding which packages can be eliminated by a minimal headless package selection.

Three multi-license expressions were correctly held at NEEDS_REVIEW rather than automatically rejected:
- `ipxe-qemu-256k-compat-efi-roms`: GPL-2+ OR UBDL.
- `libflac14`: GPL-2+ OR LGPL-2.1+.
- `librdmacm1t64`: BSD-MIT OR GPL-2.

A fourth package, `libfdt1`, remains REJECTED because its primary `Files: *` declaration is GPL-2+; a BSD notice in another file stanza does not prove that the installed library payload is selectable under BSD.

Twenty-seven package records contain LGPL declarations in their copyright metadata. They remain NEEDS_REVIEW; no LGPL approval is recorded. The QEMU package family also has a non-SPDX `GPL2-or-LGPL2-with-additions` copyright expression and remains NEEDS_REVIEW in the package-level inventory. The source-level cross-repository inventory conservatively marks the QEMU emulator as GPL from its upstream COPYING evidence. This discrepancy requires file/binary-level license mapping; QEMU remains BLOCKED and must not be treated as approved.

The four UNKNOWN package records and all 84 NEEDS_REVIEW records are detailed in `ci-audit/system-packages.json` inside the artifact. The package inventory is additional to the 29-repository source/build graph; it is not merged or deduplicated into that report.

## Remaining review

- Review Limine's binary archive contents and reconcile them with `3RDPARTY.md`.
- Verify the detached release archive signature independently.
- Replace or obtain explicit policy resolution for GPL QEMU and xorriso; do not remove them before replacement parity tests.
- Verify the final ISO contains the notices at the stated path and publish its SBOM.
- Security and maintenance status for this Limine release remain NOT_ASSESSED.

Limine is retained because it is a mature bootloader component; replacing it with an in-house bootloader would increase security and maintenance risk. No GPL/AGPL issue was identified in the reviewed upstream notices.
