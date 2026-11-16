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
echo "${LIMINE_ARCHIVE_SHA256}  $ARCHIVE" | sha256sum --check
tar -xJf "$ARCHIVE" -C "$WORK_DIR"
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

make -C "$LIMINE_DIR"
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
cp "$LIMINE_NOTICE_DIR/COPYING" "$LIMINE_NOTICE_DIR/3RDPARTY.md" "$NOTICE_DEST/"
xorriso -as mkisofs -R -r -J \
    -b boot/limine/limine-bios-cd.bin \
    -no-emul-boot -boot-load-size 4 -boot-info-table \
    "$ISO_ROOT" -o "$OUTPUT_PATH"
"$LIMINE_DIR/limine" bios-install --force "$OUTPUT_PATH"
echo "Created BIOS-bootable WICOFE ISO: $OUTPUT_PATH"
