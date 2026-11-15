/* WICOFE AArch64 higher-half kernel linker layout.
 * Physical load addresses are image-relative; Limine supplies the runtime
 * physical/virtual image bases through its executable-address response.
 */
OUTPUT_FORMAT(elf64-littleaarch64)
OUTPUT_ARCH(aarch64)
ENTRY(_start)

KERNEL_VMA = 0xffff000000080000;

PHDRS
{
    text   PT_LOAD FLAGS(5); /* R-X */
    rodata PT_LOAD FLAGS(4); /* R-- */
    data   PT_LOAD FLAGS(6); /* RW- */
}

SECTIONS
{
    . = KERNEL_VMA;
    __kernel_start = .;

    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_VMA)
    {
        KEEP(*(.text.entry))
        *(.text .text.*)
    } :text

    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_VMA)
    {
        KEEP(*(.limine_requests_start))
        KEEP(*(.limine_requests))
        KEEP(*(.limine_requests_end))
        *(.rodata .rodata.*)
    } :rodata

    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_VMA)
    {
        *(.data .data.*)
        *(.got .got.*)
    } :data

    .bss ALIGN(4K) (NOLOAD) : AT(ADDR(.bss) - KERNEL_VMA)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        . = ALIGN(4K);
        __bss_end = .;
    } :data

    . = ALIGN(4K);
    __kernel_end = .;

    /DISCARD/ :
    {
        *(.comment)
        *(.eh_frame .eh_frame_hdr)
    }
}
