#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_PATH="${1:-target/wicofe-x86_64.iso}"
LIMINE_VERSION="12.9.6"
LIMINE_ARCHIVE_SHA256="939f670e0831cc940453fd3a0ddfb2b324c0b4e3c5d0e741c190aa0d13df0103"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
mkdir -p "$(dirname "$OUTPUT_PATH")"
case "$OUTPUT_PATH" in /*) ;; *) OUTPUT_PATH="$ROOT_DIR/$OUTPUT_PATH" ;; esac
ARCHIVE="$WORK_DIR/limine-binary.tar.xz"
ARCHIVE_URL="https://github.com/Limine-Bootloader/Limine/releases/download/v${LIMINE_VERSION}/limine-binary.tar.xz"
curl --fail --location --retry 3 --output "$ARCHIVE" "$ARCHIVE_URL"
python3 - "$ARCHIVE" "$LIMINE_ARCHIVE_SHA256" <<'PY'
import hashlib
import sys
from pathlib import Path

archive = Path(sys.argv[1])
expected = sys.argv[2].lower()
digest = hashlib.sha256()
with archive.open("rb") as stream:
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        digest.update(chunk)
actual = digest.hexdigest()
if actual != expected:
    raise SystemExit(f"Limine archive SHA-256 mismatch: expected {expected}, got {actual}")
PY
python3 - "$ARCHIVE" "$WORK_DIR" <<'PY'
import sys
import tarfile
from pathlib import Path

archive = Path(sys.argv[1])
destination = Path(sys.argv[2]).resolve()
with tarfile.open(archive, "r:xz") as bundle:
    for member in bundle.getmembers():
        target = (destination / member.name).resolve()
        if target != destination and destination not in target.parents:
            raise SystemExit(f"Unsafe path in Limine archive: {member.name}")
    bundle.extractall(destination, filter="data")
PY
LIMINE_DIR="$WORK_DIR/limine-binary"
for required in limine-bios.sys limine-bios-cd.bin; do
    if [[ ! -e "$LIMINE_DIR/$required" ]]; then
        echo "Missing Limine release file: $required" >&2
        exit 1
    fi
done

LIMINE_NOTICE_DIR="$ROOT_DIR/third_party/licenses/limine-12.9.6"
for required in COPYING 3RDPARTY.md; do
    if [[ ! -s "$LIMINE_NOTICE_DIR/$required" ]]; then
        echo "Missing required Limine distribution notice: $LIMINE_NOTICE_DIR/$required" >&2
        exit 1
    fi
done

if [[ ! -f "$LIMINE_DIR/limine.c" || ! -f "$LIMINE_DIR/limine-bios-hdd.h" ]]; then
    echo "Pinned Limine archive is missing host utility source/header" >&2
    find "$LIMINE_DIR" -maxdepth 2 -type f -printf '%P\n' >&2
    exit 1
fi
CC="${CC:-clang-22}"
if ! command -v "$CC" >/dev/null 2>&1; then
    echo "Required C99 compiler not found: $CC" >&2
    exit 1
fi
"$CC" -g -O2 -pipe -std=c99 -D_FILE_OFFSET_BITS=64 \
    "$LIMINE_DIR/limine.c" -o "$LIMINE_DIR/limine"
if [[ ! -x "$LIMINE_DIR/limine" ]]; then
    echo "Limine host utility was not built" >&2
    exit 1
fi
ISO_ROOT="$WORK_DIR/iso_root"
mkdir -p "$ISO_ROOT/boot/limine"
cp "$ROOT_DIR/target/x86_64-unknown-none/debug/wicofe-kernel" "$ISO_ROOT/boot/wicofe-kernel"
cp "$ROOT_DIR/limine/limine.conf" "$ISO_ROOT/boot/limine/limine.conf"
cp "$LIMINE_DIR/limine-bios.sys" "$LIMINE_DIR/limine-bios-cd.bin" "$ISO_ROOT/boot/limine/"
NOTICE_DEST="$ISO_ROOT/boot/limine/licenses/limine-12.9.6"
mkdir -p "$NOTICE_DEST"
if [[ -s "$LIMINE_DIR/3RDPARTY.md" ]] && ! cmp -s "$LIMINE_DIR/3RDPARTY.md" "$LIMINE_NOTICE_DIR/3RDPARTY.md"; then
    echo "Limine archive third-party notices differ from the pinned source notice" >&2
    exit 1
fi
cp "$LIMINE_NOTICE_DIR/COPYING" "$LIMINE_NOTICE_DIR/3RDPARTY.md" "$NOTICE_DEST/"
if [[ -s "$LIMINE_DIR/LICENSE" ]]; then
    cp "$LIMINE_DIR/LICENSE" "$NOTICE_DEST/LICENSE"
fi
xorriso -as mkisofs -R -r -J \
    -b boot/limine/limine-bios-cd.bin \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    "$ISO_ROOT" -o "$OUTPUT_PATH"
"$LIMINE_DIR/limine" bios-install --force "$OUTPUT_PATH"
echo "Created BIOS-bootable WICOFE ISO: $OUTPUT_PATH"
