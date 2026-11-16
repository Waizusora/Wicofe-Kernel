# Third-party components

WICOFE's original kernel code is licensed under MPL-2.0. Third-party components are tracked separately and retain their upstream licenses.

| Component | Version | Purpose | License | Integration |
|---|---:|---|---|---|
| [Limine](https://github.com/Limine-Bootloader/Limine) | 12.9.6 | Bootloader and ISO build tools | BSD-2-Clause | Downloaded by the ISO build script; not linked into the kernel |

The Limine binary archive is pinned by SHA-256 in `scripts/build_limine_iso.sh`. Review upstream license and third-party notices when changing the pinned version. No Limine source is copied into the kernel repository.
