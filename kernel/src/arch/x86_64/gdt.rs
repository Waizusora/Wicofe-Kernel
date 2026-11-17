//! Minimal x86-64 GDT and TSS setup for the bootstrap CPU.
//!
//! The table installs flat ring-0 code/data descriptors and a 64-bit TSS.
//! IST1 is reserved for the double-fault entry stack. This module does not
//! configure user-mode segments or per-CPU descriptor state.

use core::arch::asm;
use core::mem::size_of;

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const TSS_SELECTOR: u16 = 0x18;
pub const DOUBLE_FAULT_IST: u8 = 1;
const DOUBLE_FAULT_STACK_SIZE: usize = 16 * 1024;

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct TaskStateSegment {
    reserved0: u32,
    privilege_stack_table: [u64; 3],
    reserved1: u64,
    interrupt_stack_table: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    io_map_base: u16,
}

impl TaskStateSegment {
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            privilege_stack_table: [0; 3],
            reserved1: 0,
            interrupt_stack_table: [0; 7],
            reserved2: 0,
            reserved3: 0,
            io_map_base: size_of::<Self>() as u16,
        }
    }

    fn set_double_fault_stack(&mut self, stack_top: u64) {
        self.interrupt_stack_table[0] = stack_top;
    }

    pub const fn double_fault_stack(&self) -> u64 {
        self.interrupt_stack_table[0]
    }
}

impl Default for TaskStateSegment {
    fn default() -> Self {
        Self::new()
    }
}

#[repr(C, align(16))]
struct DoubleFaultStack([u8; DOUBLE_FAULT_STACK_SIZE]);

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

const GDT_NULL: u64 = 0;
const GDT_KERNEL_CODE: u64 = 0x00af_9a00_0000_ffff;
const GDT_KERNEL_DATA: u64 = 0x00cf_9200_0000_ffff;

const fn tss_descriptor(base: u64, limit: u32) -> [u64; 2] {
    let low = (limit as u64 & 0xffff)
        | ((base & 0x00ff_ffff) << 16)
        | (0x89_u64 << 40)
        | (((limit as u64 >> 16) & 0x0f) << 48)
        | (((base >> 24) & 0xff) << 56);
    [low, base >> 32]
}

#[cfg(target_os = "none")]
static mut BOOT_GDT: [u64; 5] = [GDT_NULL, GDT_KERNEL_CODE, GDT_KERNEL_DATA, 0, 0];

#[cfg(target_os = "none")]
static mut BOOT_TSS: TaskStateSegment = TaskStateSegment::new();

#[cfg(target_os = "none")]
static mut DOUBLE_FAULT_STACK: DoubleFaultStack = DoubleFaultStack([0; DOUBLE_FAULT_STACK_SIZE]);

/// Return the address range reserved for the bootstrap double-fault IST1 stack.
#[cfg(target_os = "none")]
pub fn double_fault_stack_bounds() -> (u64, u64) {
    let base = core::ptr::addr_of!(DOUBLE_FAULT_STACK) as u64;
    (base, base + DOUBLE_FAULT_STACK_SIZE as u64)
}

/// Install the bootstrap GDT, load the TSS, and configure IST1.
///
/// # Safety
/// Must run once on the bootstrap CPU at ring 0 with maskable interrupts
/// disabled. The current stack and kernel image mappings must remain valid
/// while CS is reloaded. The static GDT, TSS, and IST stack must remain mapped
/// and resident for the lifetime of this CPU. This routine is not SMP-safe.
#[cfg(target_os = "none")]
pub unsafe fn initialize() {
    let tss = core::ptr::addr_of_mut!(BOOT_TSS);
    let stack = core::ptr::addr_of_mut!(DOUBLE_FAULT_STACK);
    // SAFETY: Single bootstrap invocation exclusively owns these static objects.
    let stack_top = unsafe { (*stack).0.as_mut_ptr().add(DOUBLE_FAULT_STACK_SIZE) as u64 };
    // SAFETY: The TSS is packed; write_unaligned avoids creating an unaligned reference.
    unsafe {
        core::ptr::addr_of_mut!((*tss).interrupt_stack_table[0]).write_unaligned(stack_top);
    }

    let tss_base = tss as u64;
    let descriptor = tss_descriptor(tss_base, (size_of::<TaskStateSegment>() - 1) as u32);
    let gdt = core::ptr::addr_of_mut!(BOOT_GDT);
    // SAFETY: The GDT is exclusively initialized before LGDT.
    unsafe {
        (*gdt)[3] = descriptor[0];
        (*gdt)[4] = descriptor[1];
    }

    let pointer = DescriptorTablePointer {
        limit: (size_of::<[u64; 5]>() - 1) as u16,
        base: gdt as u64,
    };

    // SAFETY: Caller guarantees privileged bootstrap context and valid static
    // descriptor storage. Reload CS through a far return before loading data
    // selectors and TR, as required after replacing the GDT.
    unsafe {
        asm!(
            "lgdt [{gdt_pointer}]",
            "push {code_selector}",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, {data_selector}",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "mov ax, {tss_selector}",
            "ltr ax",
            gdt_pointer = in(reg) &pointer,
            code_selector = const KERNEL_CODE_SELECTOR as u64,
            data_selector = const KERNEL_DATA_SELECTOR,
            tss_selector = const TSS_SELECTOR,
            out("rax") _,
        );
    }
}

/// Return the configured bootstrap TSS for diagnostics/tests.
#[cfg(target_os = "none")]
pub fn bootstrap_tss() -> &'static TaskStateSegment {
    // SAFETY: The object is initialized once and remains resident thereafter.
    unsafe { &*core::ptr::addr_of!(BOOT_TSS) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tss_has_architectural_size_and_ist_slot() {
        assert_eq!(size_of::<TaskStateSegment>(), 104);
        let mut tss = TaskStateSegment::new();
        tss.set_double_fault_stack(0xffff_ffff_8000_4000);
        assert_eq!(tss.double_fault_stack(), 0xffff_ffff_8000_4000);
    }

    #[test]
    fn tss_descriptor_encodes_base_limit_and_available_type() {
        let base = 0xffff_ffff_8123_4560;
        let descriptor = tss_descriptor(base, 103);
        assert_eq!(descriptor[0] & 0xffff, 103);
        assert_eq!((descriptor[0] >> 40) & 0xff, 0x89);
        assert_eq!(
            ((descriptor[0] >> 16) & 0x00ff_ffff)
                | (((descriptor[0] >> 56) & 0xff) << 24)
                | (descriptor[1] << 32),
            base
        );
    }
}
