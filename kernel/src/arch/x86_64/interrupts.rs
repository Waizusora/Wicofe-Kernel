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
        if handler_address == 0
            || !is_canonical_48(handler_address)
        {
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
        *slot = IdtEntry::interrupt_gate(
            handler_address,
            code_selector,
            privilege_level,
            ist_index,
        );
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
