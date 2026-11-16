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
| SHA-256 | `939f670e75e75afdc20536c73f7e7cdbd379cab67a17c17e0bb0ed92a8ccf78c` |
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

## Remaining review

- Review Limine's binary archive contents and reconcile them with `3RDPARTY.md`.
- Verify the detached release archive signature independently.
- Audit build-host tools used by the script (`curl`, `make`, `tar`, `sha256sum`, `xorriso`) and pin the build environment.
- Verify the final ISO contains the notices at the stated path and publish its SBOM.
- Security and maintenance status for this Limine release remain NOT_ASSESSED.

Limine is retained because it is a mature bootloader component; replacing it with an in-house bootloader would increase security and maintenance risk. No GPL/AGPL issue was identified in the reviewed upstream notices.
