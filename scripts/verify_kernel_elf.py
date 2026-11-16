#!/usr/bin/env python3
"""Validate the structural contract of the linked WICOFE x86-64 kernel ELF."""

import struct
import sys
from pathlib import Path

ELF_HEADER = struct.Struct("<16sHHIQQQIHHHHHH")
PROGRAM_HEADER = struct.Struct("<IIQQQQQQ")

ET_EXEC = 2
EM_X86_64 = 62
PT_LOAD = 1
PF_X = 1
PF_W = 2
PF_R = 4
KERNEL_VMA = 0xFFFFFFFF80000000
PAGE_SIZE = 0x1000
EXPECTED_FLAGS = [PF_R | PF_X, PF_R, PF_R | PF_W]


def fail(message: str) -> None:
    raise SystemExit(f"ELF validation failed: {message}")


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: verify_kernel_elf.py <kernel-elf>")

    image = Path(sys.argv[1]).read_bytes()
    if len(image) < ELF_HEADER.size:
        fail("file is smaller than an ELF64 header")

    header = ELF_HEADER.unpack_from(image)
    ident, elf_type, machine, version, entry, phoff, _, _, ehsize, phentsize, phnum, *_ = header

    if ident[:4] != b"\x7fELF" or ident[4] != 2 or ident[5] != 1:
        fail("expected a little-endian ELF64 image")
    if elf_type != ET_EXEC or machine != EM_X86_64 or version != 1:
        fail("expected an x86-64 ET_EXEC image")
    if entry != KERNEL_VMA:
        fail(f"unexpected entry point: 0x{entry:016x}")
    if ehsize != ELF_HEADER.size or phentsize != PROGRAM_HEADER.size:
        fail("unexpected ELF/program-header structure size")
    if phnum != 3:
        fail(f"expected exactly 3 PT_LOAD segments, found {phnum}")
    if phoff + phnum * phentsize > len(image):
        fail("program-header table extends beyond end of file")

    loads = []
    for index in range(phnum):
        fields = PROGRAM_HEADER.unpack_from(image, phoff + index * phentsize)
        p_type, flags, offset, vaddr, paddr, filesz, memsz, align = fields
        if p_type != PT_LOAD:
            fail(f"program header {index} is not PT_LOAD")
        if filesz > memsz:
            fail(f"segment {index} has p_filesz greater than p_memsz")
        if offset + filesz > len(image):
            fail(f"segment {index} file range exceeds the ELF file")
        if vaddr < paddr or vaddr - paddr != KERNEL_VMA:
            fail(f"segment {index} does not follow the higher-half VMA/LMA mapping")
        if align != PAGE_SIZE or vaddr % PAGE_SIZE or paddr % PAGE_SIZE:
            fail(f"segment {index} is not aligned to 4 KiB")
        if flags & (PF_W | PF_X) == (PF_W | PF_X):
            fail(f"segment {index} is writable and executable")
        loads.append((flags, paddr, vaddr, filesz, memsz))

    if [segment[0] for segment in loads] != EXPECTED_FLAGS:
        fail(f"unexpected PT_LOAD permissions: {[segment[0] for segment in loads]}")
    if [segment[1] for segment in loads] != sorted(segment[1] for segment in loads):
        fail("PT_LOAD physical load addresses are not ordered")

    print("ELF validation passed:")
    print(f"  entry: 0x{entry:016x}")
    for index, (flags, paddr, vaddr, filesz, memsz) in enumerate(loads):
        permissions = "".join(
            letter if flags & bit else "-"
            for letter, bit in (("R", PF_R), ("W", PF_W), ("X", PF_X))
        )
        print(
            f"  LOAD {index}: {permissions} "
            f"VMA=0x{vaddr:016x} LMA=0x{paddr:016x} "
            f"file=0x{filesz:x} memory=0x{memsz:x}"
        )


if __name__ == "__main__":
    main()
