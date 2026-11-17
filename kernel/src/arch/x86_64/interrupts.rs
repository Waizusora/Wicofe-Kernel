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

// Fatal exception entry and diagnostic dispatch support.
// The stubs normalize CPU-pushed error codes and terminate through the Rust
// dispatcher. They are not installed into the IDT automatically.
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
    WICOFE_ISR_NOERR 32
    WICOFE_ISR_NOERR 33
    WICOFE_ISR_NOERR 34
    WICOFE_ISR_NOERR 35
    WICOFE_ISR_NOERR 36
    WICOFE_ISR_NOERR 37
    WICOFE_ISR_NOERR 38
    WICOFE_ISR_NOERR 39
    WICOFE_ISR_NOERR 40
    WICOFE_ISR_NOERR 41
    WICOFE_ISR_NOERR 42
    WICOFE_ISR_NOERR 43
    WICOFE_ISR_NOERR 44
    WICOFE_ISR_NOERR 45
    WICOFE_ISR_NOERR 46
    WICOFE_ISR_NOERR 47
    WICOFE_ISR_NOERR 48
    WICOFE_ISR_NOERR 49
    WICOFE_ISR_NOERR 50
    WICOFE_ISR_NOERR 51
    WICOFE_ISR_NOERR 52
    WICOFE_ISR_NOERR 53
    WICOFE_ISR_NOERR 54
    WICOFE_ISR_NOERR 55
    WICOFE_ISR_NOERR 56
    WICOFE_ISR_NOERR 57
    WICOFE_ISR_NOERR 58
    WICOFE_ISR_NOERR 59
    WICOFE_ISR_NOERR 60
    WICOFE_ISR_NOERR 61
    WICOFE_ISR_NOERR 62
    WICOFE_ISR_NOERR 63
    WICOFE_ISR_NOERR 64
    WICOFE_ISR_NOERR 65
    WICOFE_ISR_NOERR 66
    WICOFE_ISR_NOERR 67
    WICOFE_ISR_NOERR 68
    WICOFE_ISR_NOERR 69
    WICOFE_ISR_NOERR 70
    WICOFE_ISR_NOERR 71
    WICOFE_ISR_NOERR 72
    WICOFE_ISR_NOERR 73
    WICOFE_ISR_NOERR 74
    WICOFE_ISR_NOERR 75
    WICOFE_ISR_NOERR 76
    WICOFE_ISR_NOERR 77
    WICOFE_ISR_NOERR 78
    WICOFE_ISR_NOERR 79
    WICOFE_ISR_NOERR 80
    WICOFE_ISR_NOERR 81
    WICOFE_ISR_NOERR 82
    WICOFE_ISR_NOERR 83
    WICOFE_ISR_NOERR 84
    WICOFE_ISR_NOERR 85
    WICOFE_ISR_NOERR 86
    WICOFE_ISR_NOERR 87
    WICOFE_ISR_NOERR 88
    WICOFE_ISR_NOERR 89
    WICOFE_ISR_NOERR 90
    WICOFE_ISR_NOERR 91
    WICOFE_ISR_NOERR 92
    WICOFE_ISR_NOERR 93
    WICOFE_ISR_NOERR 94
    WICOFE_ISR_NOERR 95
    WICOFE_ISR_NOERR 96
    WICOFE_ISR_NOERR 97
    WICOFE_ISR_NOERR 98
    WICOFE_ISR_NOERR 99
    WICOFE_ISR_NOERR 100
    WICOFE_ISR_NOERR 101
    WICOFE_ISR_NOERR 102
    WICOFE_ISR_NOERR 103
    WICOFE_ISR_NOERR 104
    WICOFE_ISR_NOERR 105
    WICOFE_ISR_NOERR 106
    WICOFE_ISR_NOERR 107
    WICOFE_ISR_NOERR 108
    WICOFE_ISR_NOERR 109
    WICOFE_ISR_NOERR 110
    WICOFE_ISR_NOERR 111
    WICOFE_ISR_NOERR 112
    WICOFE_ISR_NOERR 113
    WICOFE_ISR_NOERR 114
    WICOFE_ISR_NOERR 115
    WICOFE_ISR_NOERR 116
    WICOFE_ISR_NOERR 117
    WICOFE_ISR_NOERR 118
    WICOFE_ISR_NOERR 119
    WICOFE_ISR_NOERR 120
    WICOFE_ISR_NOERR 121
    WICOFE_ISR_NOERR 122
    WICOFE_ISR_NOERR 123
    WICOFE_ISR_NOERR 124
    WICOFE_ISR_NOERR 125
    WICOFE_ISR_NOERR 126
    WICOFE_ISR_NOERR 127
    WICOFE_ISR_NOERR 128
    WICOFE_ISR_NOERR 129
    WICOFE_ISR_NOERR 130
    WICOFE_ISR_NOERR 131
    WICOFE_ISR_NOERR 132
    WICOFE_ISR_NOERR 133
    WICOFE_ISR_NOERR 134
    WICOFE_ISR_NOERR 135
    WICOFE_ISR_NOERR 136
    WICOFE_ISR_NOERR 137
    WICOFE_ISR_NOERR 138
    WICOFE_ISR_NOERR 139
    WICOFE_ISR_NOERR 140
    WICOFE_ISR_NOERR 141
    WICOFE_ISR_NOERR 142
    WICOFE_ISR_NOERR 143
    WICOFE_ISR_NOERR 144
    WICOFE_ISR_NOERR 145
    WICOFE_ISR_NOERR 146
    WICOFE_ISR_NOERR 147
    WICOFE_ISR_NOERR 148
    WICOFE_ISR_NOERR 149
    WICOFE_ISR_NOERR 150
    WICOFE_ISR_NOERR 151
    WICOFE_ISR_NOERR 152
    WICOFE_ISR_NOERR 153
    WICOFE_ISR_NOERR 154
    WICOFE_ISR_NOERR 155
    WICOFE_ISR_NOERR 156
    WICOFE_ISR_NOERR 157
    WICOFE_ISR_NOERR 158
    WICOFE_ISR_NOERR 159
    WICOFE_ISR_NOERR 160
    WICOFE_ISR_NOERR 161
    WICOFE_ISR_NOERR 162
    WICOFE_ISR_NOERR 163
    WICOFE_ISR_NOERR 164
    WICOFE_ISR_NOERR 165
    WICOFE_ISR_NOERR 166
    WICOFE_ISR_NOERR 167
    WICOFE_ISR_NOERR 168
    WICOFE_ISR_NOERR 169
    WICOFE_ISR_NOERR 170
    WICOFE_ISR_NOERR 171
    WICOFE_ISR_NOERR 172
    WICOFE_ISR_NOERR 173
    WICOFE_ISR_NOERR 174
    WICOFE_ISR_NOERR 175
    WICOFE_ISR_NOERR 176
    WICOFE_ISR_NOERR 177
    WICOFE_ISR_NOERR 178
    WICOFE_ISR_NOERR 179
    WICOFE_ISR_NOERR 180
    WICOFE_ISR_NOERR 181
    WICOFE_ISR_NOERR 182
    WICOFE_ISR_NOERR 183
    WICOFE_ISR_NOERR 184
    WICOFE_ISR_NOERR 185
    WICOFE_ISR_NOERR 186
    WICOFE_ISR_NOERR 187
    WICOFE_ISR_NOERR 188
    WICOFE_ISR_NOERR 189
    WICOFE_ISR_NOERR 190
    WICOFE_ISR_NOERR 191
    WICOFE_ISR_NOERR 192
    WICOFE_ISR_NOERR 193
    WICOFE_ISR_NOERR 194
    WICOFE_ISR_NOERR 195
    WICOFE_ISR_NOERR 196
    WICOFE_ISR_NOERR 197
    WICOFE_ISR_NOERR 198
    WICOFE_ISR_NOERR 199
    WICOFE_ISR_NOERR 200
    WICOFE_ISR_NOERR 201
    WICOFE_ISR_NOERR 202
    WICOFE_ISR_NOERR 203
    WICOFE_ISR_NOERR 204
    WICOFE_ISR_NOERR 205
    WICOFE_ISR_NOERR 206
    WICOFE_ISR_NOERR 207
    WICOFE_ISR_NOERR 208
    WICOFE_ISR_NOERR 209
    WICOFE_ISR_NOERR 210
    WICOFE_ISR_NOERR 211
    WICOFE_ISR_NOERR 212
    WICOFE_ISR_NOERR 213
    WICOFE_ISR_NOERR 214
    WICOFE_ISR_NOERR 215
    WICOFE_ISR_NOERR 216
    WICOFE_ISR_NOERR 217
    WICOFE_ISR_NOERR 218
    WICOFE_ISR_NOERR 219
    WICOFE_ISR_NOERR 220
    WICOFE_ISR_NOERR 221
    WICOFE_ISR_NOERR 222
    WICOFE_ISR_NOERR 223
    WICOFE_ISR_NOERR 224
    WICOFE_ISR_NOERR 225
    WICOFE_ISR_NOERR 226
    WICOFE_ISR_NOERR 227
    WICOFE_ISR_NOERR 228
    WICOFE_ISR_NOERR 229
    WICOFE_ISR_NOERR 230
    WICOFE_ISR_NOERR 231
    WICOFE_ISR_NOERR 232
    WICOFE_ISR_NOERR 233
    WICOFE_ISR_NOERR 234
    WICOFE_ISR_NOERR 235
    WICOFE_ISR_NOERR 236
    WICOFE_ISR_NOERR 237
    WICOFE_ISR_NOERR 238
    WICOFE_ISR_NOERR 239
    WICOFE_ISR_NOERR 240
    WICOFE_ISR_NOERR 241
    WICOFE_ISR_NOERR 242
    WICOFE_ISR_NOERR 243
    WICOFE_ISR_NOERR 244
    WICOFE_ISR_NOERR 245
    WICOFE_ISR_NOERR 246
    WICOFE_ISR_NOERR 247
    WICOFE_ISR_NOERR 248
    WICOFE_ISR_NOERR 249
    WICOFE_ISR_NOERR 250
    WICOFE_ISR_NOERR 251
    WICOFE_ISR_NOERR 252
    WICOFE_ISR_NOERR 253
    WICOFE_ISR_NOERR 254
    WICOFE_ISR_NOERR 255

    .section .rodata
    .balign 8
    .global wicofe_interrupt_stub_table
    .type wicofe_interrupt_stub_table,@object
wicofe_interrupt_stub_table:
    .quad wicofe_exception_32
    .quad wicofe_exception_33
    .quad wicofe_exception_34
    .quad wicofe_exception_35
    .quad wicofe_exception_36
    .quad wicofe_exception_37
    .quad wicofe_exception_38
    .quad wicofe_exception_39
    .quad wicofe_exception_40
    .quad wicofe_exception_41
    .quad wicofe_exception_42
    .quad wicofe_exception_43
    .quad wicofe_exception_44
    .quad wicofe_exception_45
    .quad wicofe_exception_46
    .quad wicofe_exception_47
    .quad wicofe_exception_48
    .quad wicofe_exception_49
    .quad wicofe_exception_50
    .quad wicofe_exception_51
    .quad wicofe_exception_52
    .quad wicofe_exception_53
    .quad wicofe_exception_54
    .quad wicofe_exception_55
    .quad wicofe_exception_56
    .quad wicofe_exception_57
    .quad wicofe_exception_58
    .quad wicofe_exception_59
    .quad wicofe_exception_60
    .quad wicofe_exception_61
    .quad wicofe_exception_62
    .quad wicofe_exception_63
    .quad wicofe_exception_64
    .quad wicofe_exception_65
    .quad wicofe_exception_66
    .quad wicofe_exception_67
    .quad wicofe_exception_68
    .quad wicofe_exception_69
    .quad wicofe_exception_70
    .quad wicofe_exception_71
    .quad wicofe_exception_72
    .quad wicofe_exception_73
    .quad wicofe_exception_74
    .quad wicofe_exception_75
    .quad wicofe_exception_76
    .quad wicofe_exception_77
    .quad wicofe_exception_78
    .quad wicofe_exception_79
    .quad wicofe_exception_80
    .quad wicofe_exception_81
    .quad wicofe_exception_82
    .quad wicofe_exception_83
    .quad wicofe_exception_84
    .quad wicofe_exception_85
    .quad wicofe_exception_86
    .quad wicofe_exception_87
    .quad wicofe_exception_88
    .quad wicofe_exception_89
    .quad wicofe_exception_90
    .quad wicofe_exception_91
    .quad wicofe_exception_92
    .quad wicofe_exception_93
    .quad wicofe_exception_94
    .quad wicofe_exception_95
    .quad wicofe_exception_96
    .quad wicofe_exception_97
    .quad wicofe_exception_98
    .quad wicofe_exception_99
    .quad wicofe_exception_100
    .quad wicofe_exception_101
    .quad wicofe_exception_102
    .quad wicofe_exception_103
    .quad wicofe_exception_104
    .quad wicofe_exception_105
    .quad wicofe_exception_106
    .quad wicofe_exception_107
    .quad wicofe_exception_108
    .quad wicofe_exception_109
    .quad wicofe_exception_110
    .quad wicofe_exception_111
    .quad wicofe_exception_112
    .quad wicofe_exception_113
    .quad wicofe_exception_114
    .quad wicofe_exception_115
    .quad wicofe_exception_116
    .quad wicofe_exception_117
    .quad wicofe_exception_118
    .quad wicofe_exception_119
    .quad wicofe_exception_120
    .quad wicofe_exception_121
    .quad wicofe_exception_122
    .quad wicofe_exception_123
    .quad wicofe_exception_124
    .quad wicofe_exception_125
    .quad wicofe_exception_126
    .quad wicofe_exception_127
    .quad wicofe_exception_128
    .quad wicofe_exception_129
    .quad wicofe_exception_130
    .quad wicofe_exception_131
    .quad wicofe_exception_132
    .quad wicofe_exception_133
    .quad wicofe_exception_134
    .quad wicofe_exception_135
    .quad wicofe_exception_136
    .quad wicofe_exception_137
    .quad wicofe_exception_138
    .quad wicofe_exception_139
    .quad wicofe_exception_140
    .quad wicofe_exception_141
    .quad wicofe_exception_142
    .quad wicofe_exception_143
    .quad wicofe_exception_144
    .quad wicofe_exception_145
    .quad wicofe_exception_146
    .quad wicofe_exception_147
    .quad wicofe_exception_148
    .quad wicofe_exception_149
    .quad wicofe_exception_150
    .quad wicofe_exception_151
    .quad wicofe_exception_152
    .quad wicofe_exception_153
    .quad wicofe_exception_154
    .quad wicofe_exception_155
    .quad wicofe_exception_156
    .quad wicofe_exception_157
    .quad wicofe_exception_158
    .quad wicofe_exception_159
    .quad wicofe_exception_160
    .quad wicofe_exception_161
    .quad wicofe_exception_162
    .quad wicofe_exception_163
    .quad wicofe_exception_164
    .quad wicofe_exception_165
    .quad wicofe_exception_166
    .quad wicofe_exception_167
    .quad wicofe_exception_168
    .quad wicofe_exception_169
    .quad wicofe_exception_170
    .quad wicofe_exception_171
    .quad wicofe_exception_172
    .quad wicofe_exception_173
    .quad wicofe_exception_174
    .quad wicofe_exception_175
    .quad wicofe_exception_176
    .quad wicofe_exception_177
    .quad wicofe_exception_178
    .quad wicofe_exception_179
    .quad wicofe_exception_180
    .quad wicofe_exception_181
    .quad wicofe_exception_182
    .quad wicofe_exception_183
    .quad wicofe_exception_184
    .quad wicofe_exception_185
    .quad wicofe_exception_186
    .quad wicofe_exception_187
    .quad wicofe_exception_188
    .quad wicofe_exception_189
    .quad wicofe_exception_190
    .quad wicofe_exception_191
    .quad wicofe_exception_192
    .quad wicofe_exception_193
    .quad wicofe_exception_194
    .quad wicofe_exception_195
    .quad wicofe_exception_196
    .quad wicofe_exception_197
    .quad wicofe_exception_198
    .quad wicofe_exception_199
    .quad wicofe_exception_200
    .quad wicofe_exception_201
    .quad wicofe_exception_202
    .quad wicofe_exception_203
    .quad wicofe_exception_204
    .quad wicofe_exception_205
    .quad wicofe_exception_206
    .quad wicofe_exception_207
    .quad wicofe_exception_208
    .quad wicofe_exception_209
    .quad wicofe_exception_210
    .quad wicofe_exception_211
    .quad wicofe_exception_212
    .quad wicofe_exception_213
    .quad wicofe_exception_214
    .quad wicofe_exception_215
    .quad wicofe_exception_216
    .quad wicofe_exception_217
    .quad wicofe_exception_218
    .quad wicofe_exception_219
    .quad wicofe_exception_220
    .quad wicofe_exception_221
    .quad wicofe_exception_222
    .quad wicofe_exception_223
    .quad wicofe_exception_224
    .quad wicofe_exception_225
    .quad wicofe_exception_226
    .quad wicofe_exception_227
    .quad wicofe_exception_228
    .quad wicofe_exception_229
    .quad wicofe_exception_230
    .quad wicofe_exception_231
    .quad wicofe_exception_232
    .quad wicofe_exception_233
    .quad wicofe_exception_234
    .quad wicofe_exception_235
    .quad wicofe_exception_236
    .quad wicofe_exception_237
    .quad wicofe_exception_238
    .quad wicofe_exception_239
    .quad wicofe_exception_240
    .quad wicofe_exception_241
    .quad wicofe_exception_242
    .quad wicofe_exception_243
    .quad wicofe_exception_244
    .quad wicofe_exception_245
    .quad wicofe_exception_246
    .quad wicofe_exception_247
    .quad wicofe_exception_248
    .quad wicofe_exception_249
    .quad wicofe_exception_250
    .quad wicofe_exception_251
    .quad wicofe_exception_252
    .quad wicofe_exception_253
    .quad wicofe_exception_254
    .quad wicofe_exception_255
    .size wicofe_interrupt_stub_table, . - wicofe_interrupt_stub_table
    .section .text
    "#,
    options(att_syntax)
);

/// Boot-time IDT storage. It remains resident for the lifetime of the kernel.
#[cfg(target_os = "none")]
static mut BOOT_IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

#[cfg(target_os = "none")]
unsafe extern "C" {
    static wicofe_interrupt_stub_table: [u64; 224];
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
/// All 256 vectors use distinct normalized entry stubs. Vectors 0-31 are
/// CPU exception stubs; vectors 32-255 are terminal interrupt stubs that
/// preserve the actual vector number until device-specific handlers exist.
///
/// # Safety
/// Must run once on the bootstrap CPU at ring 0, with interrupts disabled.
/// The currently loaded GDT must remain active and its current CS selector
/// must identify a present 64-bit code segment. The kernel image and this
/// static IDT must be mapped executable/readable respectively. The current
/// stack must be valid for fatal exception entry. This baseline does not use
/// IST1 is reserved for double fault and must be configured by the TSS before
/// this IDT is loaded.
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
    // SAFETY: The assembly exports exactly 224 pointer-sized entries for
    // vectors 32-255; each points to its corresponding normalized stub.
    let interrupt_handlers = unsafe { core::ptr::addr_of!(wicofe_interrupt_stub_table) };

    // SAFETY: The function's single-call contract grants exclusive access to
    // this static table before it is installed on the bootstrap CPU.
    let idt = unsafe { &mut *core::ptr::addr_of_mut!(BOOT_IDT) };
    for vector in 0..IDT_ENTRIES {
        let handler = if vector < exception_handlers.len() {
            exception_handlers[vector]
        } else {
            // SAFETY: vector is in 32..256, so the table index is in 0..224.
            unsafe { interrupt_handlers.cast::<u64>().add(vector - 32).read() }
        };
        let ist = if vector == 8 {
            crate::arch::x86_64::gdt::DOUBLE_FAULT_IST
        } else {
            0
        };
        idt.set_handler(vector as u8, handler, code_selector, 0, ist)?;
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
