//! x86-64 legacy IRQ dispatch and handler registration.
//!
//! IRQ vectors 32..47 have a returning IRETQ entry path. PIC lines remain
//! masked until a handler is registered and explicitly unmasked.

use super::pic;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const VECTOR_BASE: u8 = pic::MASTER_VECTOR_OFFSET;
pub const VECTOR_LIMIT: u8 = VECTOR_BASE + pic::IRQ_COUNT;
pub const HANDLER_COUNT: usize = pic::IRQ_COUNT as usize;

/// CPU register and hardware frame saved by the IRQ assembly entry path.
#[repr(C)]
pub struct IrqStackFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub vector: u64,
    pub error_code: u64,
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub rflags: u64,
}

/// IRQ callback runs with IF clear on the interrupted CPU.
pub type IrqHandler = extern "C" fn(&mut IrqStackFrame);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrqError {
    InvalidIrq(u8),
    HandlerAlreadyRegistered(u8),
    HandlerNotRegistered(u8),
}

/// Fixed-size atomic handler table. Handlers are write-once in this stage.
pub struct HandlerTable {
    handlers: [AtomicUsize; HANDLER_COUNT],
}

impl HandlerTable {
    pub const fn new() -> Self {
        Self {
            handlers: [const { AtomicUsize::new(0) }; HANDLER_COUNT],
        }
    }

    pub fn register(&self, irq: u8, handler: IrqHandler) -> Result<(), IrqError> {
        let slot = self
            .handlers
            .get(irq as usize)
            .ok_or(IrqError::InvalidIrq(irq))?;
        slot.compare_exchange(0, handler as usize, Ordering::Release, Ordering::Relaxed)
            .map(|_| ())
            .map_err(|_| IrqError::HandlerAlreadyRegistered(irq))
    }

    fn get(&self, irq: u8) -> Option<IrqHandler> {
        let address = self.handlers.get(irq as usize)?.load(Ordering::Acquire);
        if address == 0 {
            None
        } else {
            // SAFETY: register() stores valid write-once function pointers.
            Some(unsafe { core::mem::transmute::<usize, IrqHandler>(address) })
        }
    }
}

impl Default for HandlerTable {
    fn default() -> Self {
        Self::new()
    }
}

static HANDLERS: HandlerTable = HandlerTable::new();

/// Register a handler without enabling its hardware IRQ line.
pub fn register_handler(irq: u8, handler: IrqHandler) -> Result<(), IrqError> {
    HANDLERS.register(irq, handler)
}

/// Explicitly unmask a registered IRQ. This never executes STI.
///
/// # Safety
/// Call at ring 0 with CPU interrupts disabled and exclusive PIC ownership.
#[cfg(target_os = "none")]
pub unsafe fn unmask(irq: u8) -> Result<(), IrqError> {
    if irq >= pic::IRQ_COUNT {
        return Err(IrqError::InvalidIrq(irq));
    }
    if HANDLERS.get(irq).is_none() {
        return Err(IrqError::HandlerNotRegistered(irq));
    }
    if irq >= 8 {
        // SAFETY: Caller guarantees serialized PIC access and IF=0.
        unsafe { pic::set_irq_mask(2, false) }.map_err(|_| IrqError::InvalidIrq(2))?;
    }
    // SAFETY: Caller guarantees serialized PIC access and IF=0.
    unsafe { pic::set_irq_mask(irq, false) }.map_err(|_| IrqError::InvalidIrq(irq))?;
    Ok(())
}

/// Returning IRQ dispatcher called by the assembly entry path.
///
/// # Safety
/// The pointer must reference the complete live frame created by the IRQ stub.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wicofe_irq_dispatch(vector: u64, frame: *mut IrqStackFrame) {
    if !(VECTOR_BASE as u64..VECTOR_LIMIT as u64).contains(&vector) || frame.is_null() {
        panic!("invalid hardware IRQ frame: vector={vector}");
    }
    let irq = vector as u8 - VECTOR_BASE;
    // SAFETY: IRQ entry runs at ring 0 with IF clear and serializes PIC access.
    let spurious =
        unsafe { pic::is_spurious_irq(irq) }.unwrap_or_else(|_| panic!("invalid PIC IRQ {irq}"));
    if spurious {
        return;
    }
    let handler = HANDLERS
        .get(irq)
        .unwrap_or_else(|| panic!("unhandled hardware IRQ {irq}"));
    // SAFETY: Assembly supplies the live frame with the declared layout.
    unsafe { handler(&mut *frame) };
    // SAFETY: The real IRQ has been serviced; PIC access remains serialized.
    unsafe { pic::send_eoi(irq) }.unwrap_or_else(|_| panic!("failed to acknowledge IRQ {irq}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    extern "C" fn test_handler(_: &mut IrqStackFrame) {}

    #[test]
    fn frame_matches_assembly_push_order() {
        assert_eq!(size_of::<IrqStackFrame>(), 20 * size_of::<u64>());
        assert_eq!(offset_of!(IrqStackFrame, r15), 0);
        assert_eq!(offset_of!(IrqStackFrame, rax), 14 * 8);
        assert_eq!(offset_of!(IrqStackFrame, vector), 15 * 8);
        assert_eq!(offset_of!(IrqStackFrame, error_code), 16 * 8);
        assert_eq!(offset_of!(IrqStackFrame, instruction_pointer), 17 * 8);
        assert_eq!(offset_of!(IrqStackFrame, rflags), 19 * 8);
    }

    #[test]
    fn handler_registration_is_write_once() {
        let table = HandlerTable::new();
        assert_eq!(table.register(0, test_handler), Ok(()));
        assert_eq!(
            table.register(0, test_handler),
            Err(IrqError::HandlerAlreadyRegistered(0))
        );
        assert!(table.get(0).is_some());
    }

    #[test]
    fn invalid_irq_is_rejected() {
        let table = HandlerTable::new();
        assert_eq!(
            table.register(16, test_handler),
            Err(IrqError::InvalidIrq(16))
        );
    }

    #[test]
    fn handler_table_rejects_missing_entries() {
        assert!(HandlerTable::new().get(3).is_none());
    }
}
