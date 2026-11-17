//! x86-64 Interrupt Descriptor Table construction and activation.
//!
//! This module builds a complete 256-entry IDT but does not install handlers
//! or activate the table during boot. Handler addresses must point to assembly
//! entry stubs that obey the x86-64 interrupt stack/return ABI.

use core::arch::asm;
use core::mem::size_of;

const IDT_ENTRIES: usize = 256;
const INTERRUPT_GATE: u8 = 0x0e;
const PRESENT: u8 = 1 << 7;

/// Errors returned while constructing or loading an IDT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdtError {
    InvalidHandlerAddress,
    InvalidCodeSelector,
    InvalidPrivilegeLevel,
    InvalidIstIndex,
    IncompleteTable,
}

/// One 16-byte x86-64 IDT gate descriptor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C, align(16))]
pub struct IdtEntry {
    low: u64,
    high: u64,
}

impl IdtEntry {
    const MISSING: Self = Self { low: 0, high: 0 };

    /// Return whether the descriptor is present.
    pub const fn is_present(self) -> bool {
        (self.low >> 47) & 1 != 0
    }

    /// Decode the handler offset from the descriptor.
    pub const fn handler_address(self) -> u64 {
        (self.low & 0xffff)
            | (((self.low >> 48) & 0xffff) << 16)
            | ((self.high & 0xffff_ffff) << 32)
    }

    /// Decode the code-segment selector.
    pub const fn code_selector(self) -> u16 {
        ((self.low >> 16) & 0xffff) as u16
    }

    /// Decode the interrupt stack-table index.
    pub const fn ist_index(self) -> u8 {
        ((self.low >> 32) & 0x7) as u8
    }

    /// Decode the descriptor privilege level.
    pub const fn privilege_level(self) -> u8 {
        ((self.low >> 45) & 0x3) as u8
    }

    fn interrupt_gate(handler: u64, selector: u16, dpl: u8, ist: u8) -> Self {
        let attributes = PRESENT | (dpl << 5) | INTERRUPT_GATE;
        let low = (handler & 0xffff)
            | ((selector as u64) << 16)
            | ((ist as u64) << 32)
            | ((attributes as u64) << 40)
            | (((handler >> 16) & 0xffff) << 48);
        Self {
            low,
            high: handler >> 32,
        }
    }
}

/// A complete, 16-byte-aligned x86-64 interrupt descriptor table.
#[repr(C, align(16))]
pub struct InterruptDescriptorTable {
    entries: [IdtEntry; IDT_ENTRIES],
    populated: usize,
}

impl InterruptDescriptorTable {
    /// Create an empty IDT. Every vector must be explicitly installed before
    /// the table can be loaded.
    pub const fn new() -> Self {
        Self {
            entries: [IdtEntry::MISSING; IDT_ENTRIES],
            populated: 0,
        }
    }

    /// Install an interrupt gate for one vector.
    ///
    /// The handler must be an assembly entry stub compatible with hardware
    /// interrupt delivery. It must preserve the required registers, normalize
    /// exception error-code stack layouts, and return with IRETQ.
    pub fn set_handler(
        &mut self,
        vector: u8,
        handler_address: u64,
        code_selector: u16,
        privilege_level: u8,
        ist_index: u8,
    ) -> Result<(), IdtError> {
        if handler_address == 0 || !is_canonical_48(handler_address) {
            return Err(IdtError::InvalidHandlerAddress);
        }
        if code_selector == 0 {
            return Err(IdtError::InvalidCodeSelector);
        }
        if privilege_level > 3 {
            return Err(IdtError::InvalidPrivilegeLevel);
        }
        if ist_index > 7 {
            return Err(IdtError::InvalidIstIndex);
        }

        let slot = &mut self.entries[vector as usize];
        if !slot.is_present() {
            self.populated += 1;
        }
        *slot =
            IdtEntry::interrupt_gate(handler_address, code_selector, privilege_level, ist_index);
        Ok(())
    }

    /// Return the descriptor for a vector.
    pub const fn entry(&self, vector: u8) -> IdtEntry {
        self.entries[vector as usize]
    }

    /// Return the number of installed vectors.
    pub const fn populated_entries(&self) -> usize {
        self.populated
    }

    /// Load this complete table into the current CPU with LIDT.
    ///
    /// # Safety
    /// Every handler must be executable, mapped, and ABI-correct. The code
    /// selector must reference a present 64-bit code segment in the active
    /// GDT. Any selected IST stack must be valid and mapped. The caller must
    /// ensure that loading this IDT is safe for the current execution context.
    /// This operation is privileged and must execute at ring 0.
    pub unsafe fn load(&self) -> Result<(), IdtError> {
        if self.populated != IDT_ENTRIES {
            return Err(IdtError::IncompleteTable);
        }

        let pointer = IdtPointer {
            limit: (size_of::<[IdtEntry; IDT_ENTRIES]>() - 1) as u16,
            base: self.entries.as_ptr() as u64,
        };

        // SAFETY: The caller guarantees ring-0 execution and valid descriptors.
        unsafe {
            asm!(
                "lidt [{}]",
                in(reg) &pointer,
                options(readonly, nostack, preserves_flags)
            );
        }
        Ok(())
    }
}

impl Default for InterruptDescriptorTable {
    fn default() -> Self {
        Self::new()
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

const fn is_canonical_48(address: u64) -> bool {
    let upper = address >> 48;
    let sign = (address >> 47) & 1;
    if sign == 0 {
        upper == 0
    } else {
        upper == 0xffff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_descriptor_encodes_and_decodes_all_fields() {
        let handler = 0xffff_ffff_8123_4567;
        let entry = IdtEntry::interrupt_gate(handler, 0x08, 3, 5);

        assert!(entry.is_present());
        assert_eq!(entry.handler_address(), handler);
        assert_eq!(entry.code_selector(), 0x08);
        assert_eq!(entry.privilege_level(), 3);
        assert_eq!(entry.ist_index(), 5);
    }

    #[test]
    fn idt_tracks_replaced_and_new_vectors() {
        let mut idt = InterruptDescriptorTable::new();
        assert_eq!(idt.populated_entries(), 0);

        idt.set_handler(14, 0xffff_ffff_8000_1000, 0x08, 0, 0)
            .unwrap();
        idt.set_handler(14, 0xffff_ffff_8000_2000, 0x08, 0, 1)
            .unwrap();
        idt.set_handler(32, 0xffff_ffff_8000_3000, 0x08, 0, 0)
            .unwrap();

        assert_eq!(idt.populated_entries(), 2);
        assert_eq!(idt.entry(14).handler_address(), 0xffff_ffff_8000_2000);
        assert_eq!(idt.entry(14).ist_index(), 1);
    }

    #[test]
    fn idt_rejects_invalid_gate_parameters() {
        let mut idt = InterruptDescriptorTable::new();
        assert_eq!(
            idt.set_handler(0, 0x0000_8000_0000_0000, 0x08, 0, 0),
            Err(IdtError::InvalidHandlerAddress)
        );
        assert_eq!(
            idt.set_handler(0, 0xffff_ffff_8000_0000, 0, 0, 0),
            Err(IdtError::InvalidCodeSelector)
        );
        assert_eq!(
            idt.set_handler(0, 0xffff_ffff_8000_0000, 0x08, 4, 0),
            Err(IdtError::InvalidPrivilegeLevel)
        );
        assert_eq!(
            idt.set_handler(0, 0xffff_ffff_8000_0000, 0x08, 0, 8),
            Err(IdtError::InvalidIstIndex)
        );
    }
}

/// Fatal exception entry and diagnostic dispatch support.
///
/// The stubs normalize CPU-pushed error codes and terminate through the Rust
/// dispatcher. They are not installed into the IDT automatically.
#[cfg(target_os = "none")]
core::arch::global_asm!(
    r#"
    .section .text
    .balign 16
    .macro WICOFE_ISR_NOERR number
    .global wicofe_exception_\number
    .type wicofe_exception_\number,@function
wicofe_exception_\number:
    pushq $0
    pushq $\number
    jmp wicofe_exception_common
    .size wicofe_exception_\number, . - wicofe_exception_\number
    .endm

    .macro WICOFE_ISR_ERR number
    .global wicofe_exception_\number
    .type wicofe_exception_\number,@function
wicofe_exception_\number:
    pushq $\number
    jmp wicofe_exception_common
    .size wicofe_exception_\number, . - wicofe_exception_\number
    .endm

    .global wicofe_exception_common
    .type wicofe_exception_common,@function
wicofe_exception_common:
    cld
    pushq %rax
    pushq %rbx
    pushq %rcx
    pushq %rdx
    pushq %rbp
    pushq %rsi
    pushq %rdi
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r11
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15

    movq 120(%rsp), %rdi
    movq 128(%rsp), %rsi
    movq %rsp, %rdx
    andq $-16, %rsp
    call wicofe_exception_dispatch
1:
    cli
    hlt
    jmp 1b
    .size wicofe_exception_common, . - wicofe_exception_common

    .global wicofe_unhandled_interrupt
    .type wicofe_unhandled_interrupt,@function
wicofe_unhandled_interrupt:
    pushq $0
    pushq $255
    jmp wicofe_exception_common
    .size wicofe_unhandled_interrupt, . - wicofe_unhandled_interrupt

    WICOFE_ISR_NOERR 0
    WICOFE_ISR_NOERR 1
    WICOFE_ISR_NOERR 2
    WICOFE_ISR_NOERR 3
    WICOFE_ISR_NOERR 4
    WICOFE_ISR_NOERR 5
    WICOFE_ISR_NOERR 6
    WICOFE_ISR_NOERR 7
    WICOFE_ISR_ERR   8
    WICOFE_ISR_NOERR 9
    WICOFE_ISR_ERR   10
    WICOFE_ISR_ERR   11
    WICOFE_ISR_ERR   12
    WICOFE_ISR_ERR   13
    WICOFE_ISR_ERR   14
    WICOFE_ISR_NOERR 15
    WICOFE_ISR_NOERR 16
    WICOFE_ISR_ERR   17
    WICOFE_ISR_NOERR 18
    WICOFE_ISR_NOERR 19
    WICOFE_ISR_NOERR 20
    WICOFE_ISR_ERR   21
    WICOFE_ISR_NOERR 22
    WICOFE_ISR_NOERR 23
    WICOFE_ISR_NOERR 24
    WICOFE_ISR_NOERR 25
    WICOFE_ISR_NOERR 26
    WICOFE_ISR_NOERR 27
    WICOFE_ISR_NOERR 28
    WICOFE_ISR_ERR   29
    WICOFE_ISR_ERR   30
    WICOFE_ISR_NOERR 31
    "#,
    options(att_syntax)
);

/// Boot-time IDT storage. It remains resident for the lifetime of the kernel.
#[cfg(target_os = "none")]
static mut BOOT_IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

#[cfg(target_os = "none")]
unsafe extern "C" {
    fn wicofe_unhandled_interrupt();
    fn wicofe_exception_0();
    fn wicofe_exception_1();
    fn wicofe_exception_2();
    fn wicofe_exception_3();
    fn wicofe_exception_4();
    fn wicofe_exception_5();
    fn wicofe_exception_6();
    fn wicofe_exception_7();
    fn wicofe_exception_8();
    fn wicofe_exception_9();
    fn wicofe_exception_10();
    fn wicofe_exception_11();
    fn wicofe_exception_12();
    fn wicofe_exception_13();
    fn wicofe_exception_14();
    fn wicofe_exception_15();
    fn wicofe_exception_16();
    fn wicofe_exception_17();
    fn wicofe_exception_18();
    fn wicofe_exception_19();
    fn wicofe_exception_20();
    fn wicofe_exception_21();
    fn wicofe_exception_22();
    fn wicofe_exception_23();
    fn wicofe_exception_24();
    fn wicofe_exception_25();
    fn wicofe_exception_26();
    fn wicofe_exception_27();
    fn wicofe_exception_28();
    fn wicofe_exception_29();
    fn wicofe_exception_30();
    fn wicofe_exception_31();
}

/// Populate and load the complete fail-stop IDT on the bootstrap CPU.
///
/// All vectors 0-31 use their normalized exception stubs. Vectors 32-255
/// share a terminal fallback stub until device-specific interrupt handlers
/// are introduced. The fallback reports vector 255 as an "unhandled" marker.
///
/// # Safety
/// Must run once on the bootstrap CPU at ring 0, with interrupts disabled.
/// The currently loaded GDT must remain active and its current CS selector
/// must identify a present 64-bit code segment. The kernel image and this
/// static IDT must be mapped executable/readable respectively. The current
/// stack must be valid for fatal exception entry. This baseline does not use
/// IST; a later TSS stage must provide a dedicated double-fault stack before
/// relying on recovery from stack-corruption faults.
#[cfg(target_os = "none")]
pub unsafe fn initialize_and_load() -> Result<(), IdtError> {
    let code_selector: u16;
    // SAFETY: Reading CS is unprivileged and does not modify architectural state.
    unsafe {
        asm!(
            "mov {selector:x}, cs",
            selector = out(reg) code_selector,
            options(nomem, nostack, preserves_flags)
        );
    }

    let exception_handlers: [u64; 32] = [
        wicofe_exception_0 as *const () as u64,
        wicofe_exception_1 as *const () as u64,
        wicofe_exception_2 as *const () as u64,
        wicofe_exception_3 as *const () as u64,
        wicofe_exception_4 as *const () as u64,
        wicofe_exception_5 as *const () as u64,
        wicofe_exception_6 as *const () as u64,
        wicofe_exception_7 as *const () as u64,
        wicofe_exception_8 as *const () as u64,
        wicofe_exception_9 as *const () as u64,
        wicofe_exception_10 as *const () as u64,
        wicofe_exception_11 as *const () as u64,
        wicofe_exception_12 as *const () as u64,
        wicofe_exception_13 as *const () as u64,
        wicofe_exception_14 as *const () as u64,
        wicofe_exception_15 as *const () as u64,
        wicofe_exception_16 as *const () as u64,
        wicofe_exception_17 as *const () as u64,
        wicofe_exception_18 as *const () as u64,
        wicofe_exception_19 as *const () as u64,
        wicofe_exception_20 as *const () as u64,
        wicofe_exception_21 as *const () as u64,
        wicofe_exception_22 as *const () as u64,
        wicofe_exception_23 as *const () as u64,
        wicofe_exception_24 as *const () as u64,
        wicofe_exception_25 as *const () as u64,
        wicofe_exception_26 as *const () as u64,
        wicofe_exception_27 as *const () as u64,
        wicofe_exception_28 as *const () as u64,
        wicofe_exception_29 as *const () as u64,
        wicofe_exception_30 as *const () as u64,
        wicofe_exception_31 as *const () as u64,
    ];
    let fallback_handler = wicofe_unhandled_interrupt as *const () as u64;

    // SAFETY: The function's single-call contract grants exclusive access to
    // this static table before it is installed on the bootstrap CPU.
    let idt = unsafe { &mut *core::ptr::addr_of_mut!(BOOT_IDT) };
    for vector in 0..IDT_ENTRIES {
        let handler = if vector < exception_handlers.len() {
            exception_handlers[vector]
        } else {
            fallback_handler
        };
        idt.set_handler(vector as u8, handler, code_selector, 0, 0)?;
    }

    // SAFETY: The caller guarantees the GDT selector, mappings, stack, and
    // privileged execution context required by LIDT.
    unsafe { idt.load() }
}

/// Register/exception data passed by the x86-64 assembly entry path.
///
/// The saved GPR order is r15..rax, followed by vector, error code, RIP,
/// CS, and RFLAGS.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn wicofe_exception_dispatch(
    vector: u64,
    error_code: u64,
    registers: *const u64,
) -> ! {
    // SAFETY: The assembly entry has pushed the normalized vector/error pair
    // and all GPRs. The saved instruction pointer is the eighteenth u64.
    let instruction_pointer = unsafe { *registers.add(17) };
    panic!("CPU exception: vector={vector}, error={error_code:#x}, rip={instruction_pointer:#x}");
}
