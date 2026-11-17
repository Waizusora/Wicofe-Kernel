//! Legacy dual 8259 Programmable Interrupt Controller support.
//!
//! The controller is remapped away from CPU exception vectors and left fully
//! masked during bootstrap. External interrupts remain disabled until a later
//! stage installs and validates every required IRQ handler.

pub const MASTER_COMMAND: u16 = 0x20;
pub const MASTER_DATA: u16 = 0x21;
pub const SLAVE_COMMAND: u16 = 0xa0;
pub const SLAVE_DATA: u16 = 0xa1;
pub const MASTER_VECTOR_OFFSET: u8 = 0x20;
pub const SLAVE_VECTOR_OFFSET: u8 = 0x28;
pub const IRQ_COUNT: u8 = 16;
pub const MASK_ALL: u8 = 0xff;

const ICW1_INIT: u8 = 0x11;
const ICW4_8086: u8 = 0x01;
const OCW3_READ_ISR: u8 = 0x0b;
const EOI: u8 = 0x20;
const CASCADE_ID_MASTER: u8 = 0x04;
const CASCADE_ID_SLAVE: u8 = 0x02;

/// Port-I/O operations used by the PIC driver and its deterministic tests.
pub trait PicIo {
    /// Read one byte from an I/O port.
    fn read_u8(&mut self, port: u16) -> u8;

    /// Write one byte to an I/O port.
    fn write_u8(&mut self, port: u16, value: u8);
}

/// PIC masks observed after initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PicMasks {
    pub master: u8,
    pub slave: u8,
}

/// Result of acknowledging a legacy IRQ.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrqAcknowledge {
    /// A real IRQ was acknowledged and the required EOI command(s) were sent.
    Acknowledged,
    /// IRQ7/IRQ15 was spurious; only the required master cascade EOI was sent.
    Spurious,
}

/// Invalid IRQ number supplied to a PIC operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidIrq(pub u8);

#[inline]
fn io_wait<I: PicIo>(io: &mut I) {
    io.write_u8(0x80, 0);
}

/// Remap both 8259 controllers and leave every IRQ masked.
///
/// This routine does not enable CPU interrupts and does not unmask any IRQ.
pub fn initialize_with_io<I: PicIo>(io: &mut I) -> PicMasks {
    // Mask before reprogramming, then keep the masks closed after ICW setup.
    io.write_u8(MASTER_DATA, MASK_ALL);
    io.write_u8(SLAVE_DATA, MASK_ALL);
    io_wait(io);

    io.write_u8(MASTER_COMMAND, ICW1_INIT);
    io_wait(io);
    io.write_u8(SLAVE_COMMAND, ICW1_INIT);
    io_wait(io);

    io.write_u8(MASTER_DATA, MASTER_VECTOR_OFFSET);
    io_wait(io);
    io.write_u8(SLAVE_DATA, SLAVE_VECTOR_OFFSET);
    io_wait(io);

    io.write_u8(MASTER_DATA, CASCADE_ID_MASTER);
    io_wait(io);
    io.write_u8(SLAVE_DATA, CASCADE_ID_SLAVE);
    io_wait(io);

    io.write_u8(MASTER_DATA, ICW4_8086);
    io_wait(io);
    io.write_u8(SLAVE_DATA, ICW4_8086);
    io_wait(io);

    io.write_u8(MASTER_DATA, MASK_ALL);
    io.write_u8(SLAVE_DATA, MASK_ALL);

    PicMasks {
        master: io.read_u8(MASTER_DATA),
        slave: io.read_u8(SLAVE_DATA),
    }
}

/// Change one IRQ mask bit without disturbing the other IRQs.
pub fn set_irq_mask_with_io<I: PicIo>(
    io: &mut I,
    irq: u8,
    masked: bool,
) -> Result<PicMasks, InvalidIrq> {
    if irq >= IRQ_COUNT {
        return Err(InvalidIrq(irq));
    }

    let (port, bit) = if irq < 8 {
        (MASTER_DATA, irq)
    } else {
        (SLAVE_DATA, irq - 8)
    };
    let mut mask = io.read_u8(port);
    if masked {
        mask |= 1 << bit;
    } else {
        mask &= !(1 << bit);
    }
    io.write_u8(port, mask);

    Ok(PicMasks {
        master: if port == MASTER_DATA {
            mask
        } else {
            io.read_u8(MASTER_DATA)
        },
        slave: if port == SLAVE_DATA {
            mask
        } else {
            io.read_u8(SLAVE_DATA)
        },
    })
}

/// Send EOI for a real IRQ, including correct spurious IRQ7/IRQ15 handling.
/// Check the in-service register before invoking an IRQ handler.
/// Spurious IRQ15 requires a master cascade EOI; real IRQs remain in service.
pub fn is_spurious_irq_with_io<I: PicIo>(
    io: &mut I,
    irq: u8,
) -> Result<bool, InvalidIrq> {
    if irq >= IRQ_COUNT { return Err(InvalidIrq(irq)); }
    if irq == 7 {
        io.write_u8(MASTER_COMMAND, OCW3_READ_ISR);
        return Ok(io.read_u8(MASTER_COMMAND) & 0x80 == 0);
    }
    if irq == 15 {
        io.write_u8(SLAVE_COMMAND, OCW3_READ_ISR);
        if io.read_u8(SLAVE_COMMAND) & 0x80 == 0 {
            io.write_u8(MASTER_COMMAND, EOI);
            return Ok(true);
        }
    }
    Ok(false)
}

/// Send EOI after a real IRQ handler has completed.
pub fn send_eoi_with_io<I: PicIo>(io: &mut I, irq: u8) -> Result<(), InvalidIrq> {
    if irq >= IRQ_COUNT { return Err(InvalidIrq(irq)); }
    if irq >= 8 { io.write_u8(SLAVE_COMMAND, EOI); }
    io.write_u8(MASTER_COMMAND, EOI);
    Ok(())
}

/// Send EOI for a real IRQ, including correct spurious IRQ7/IRQ15 handling.
pub fn acknowledge_irq_with_io<I: PicIo>(
    io: &mut I,
    irq: u8,
) -> Result<IrqAcknowledge, InvalidIrq> {
    if is_spurious_irq_with_io(io, irq)? {
        return Ok(IrqAcknowledge::Spurious);
    }
    send_eoi_with_io(io, irq)?;
    Ok(IrqAcknowledge::Acknowledged)
}

#[cfg(target_os = "none")]
struct X86PortIo;

#[cfg(target_os = "none")]
impl PicIo for X86PortIo {
    fn read_u8(&mut self, port: u16) -> u8 {
        let value: u8;
        // SAFETY: The PIC driver is called only from ring-0 bootstrap/IRQ code.
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") port,
                out("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
    }

    fn write_u8(&mut self, port: u16, value: u8) {
        // SAFETY: The PIC driver is called only from ring-0 bootstrap/IRQ code.
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") port,
                in("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

/// Initialize the physical legacy PIC pair on x86-64 and keep all IRQs masked.
///
/// # Safety
/// Must execute at ring 0 with CPU interrupts disabled. The caller must ensure
/// no other CPU or device driver concurrently accesses the legacy PIC ports.
#[cfg(target_os = "none")]
pub unsafe fn initialize() -> PicMasks {
    initialize_with_io(&mut X86PortIo)
}

/// Set or clear one legacy IRQ mask bit on x86-64.
///
/// # Safety
/// Must execute at ring 0. Callers must serialize PIC access and must not
/// unmask an IRQ before its IDT entry and handler are ready.
#[cfg(target_os = "none")]
pub unsafe fn set_irq_mask(irq: u8, masked: bool) -> Result<PicMasks, InvalidIrq> {
    set_irq_mask_with_io(&mut X86PortIo, irq, masked)
}

/// Check whether an IRQ7/IRQ15 delivery is spurious before calling its handler.
///
/// # Safety
/// Must execute at ring 0 with serialized PIC access and interrupts disabled.
#[cfg(target_os = "none")]
pub unsafe fn is_spurious_irq(irq: u8) -> Result<bool, InvalidIrq> {
    is_spurious_irq_with_io(&mut X86PortIo, irq)
}

/// Send EOI after a real IRQ handler has completed.
///
/// # Safety
/// Must execute at ring 0 from a context that owns/serializes PIC access.
#[cfg(target_os = "none")]
pub unsafe fn send_eoi(irq: u8) -> Result<(), InvalidIrq> {
    send_eoi_with_io(&mut X86PortIo, irq)
}

/// Acknowledge one legacy IRQ on x86-64.
///
/// # Safety
/// Must execute at ring 0 from a context that owns/serializes PIC access.
#[cfg(target_os = "none")]
pub unsafe fn acknowledge_irq(irq: u8) -> Result<IrqAcknowledge, InvalidIrq> {
    acknowledge_irq_with_io(&mut X86PortIo, irq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    #[derive(Default)]
    struct MockPic {
        master_mask: u8,
        slave_mask: u8,
        master_isr: u8,
        slave_isr: u8,
        writes: Vec<(u16, u8)>,
    }

    impl PicIo for MockPic {
        fn read_u8(&mut self, port: u16) -> u8 {
            match port {
                MASTER_DATA => self.master_mask,
                SLAVE_DATA => self.slave_mask,
                MASTER_COMMAND => self.master_isr,
                SLAVE_COMMAND => self.slave_isr,
                _ => 0,
            }
        }

        fn write_u8(&mut self, port: u16, value: u8) {
            self.writes.push((port, value));
            match port {
                MASTER_DATA => self.master_mask = value,
                SLAVE_DATA => self.slave_mask = value,
                _ => {}
            }
        }
    }

    #[test]
    fn initialization_remaps_both_chips_and_masks_every_irq() {
        let mut pic = MockPic::default();
        assert_eq!(
            initialize_with_io(&mut pic),
            PicMasks {
                master: MASK_ALL,
                slave: MASK_ALL
            }
        );
        assert!(pic.writes.contains(&(MASTER_DATA, MASTER_VECTOR_OFFSET)));
        assert!(pic.writes.contains(&(SLAVE_DATA, SLAVE_VECTOR_OFFSET)));
        assert!(pic.writes.contains(&(MASTER_DATA, CASCADE_ID_MASTER)));
        assert!(pic.writes.contains(&(SLAVE_DATA, CASCADE_ID_SLAVE)));
        assert_eq!(pic.master_mask, MASK_ALL);
        assert_eq!(pic.slave_mask, MASK_ALL);
    }

    #[test]
    fn irq_mask_updates_only_the_selected_bit() {
        let mut pic = MockPic {
            master_mask: 0b1010_1010,
            slave_mask: 0b0101_0101,
            ..MockPic::default()
        };
        let masks = set_irq_mask_with_io(&mut pic, 10, false).unwrap();
        assert_eq!(masks.master, 0b1010_1010);
        assert_eq!(masks.slave, 0b0101_0001);

        let masks = set_irq_mask_with_io(&mut pic, 3, true).unwrap();
        assert_eq!(masks.master, 0b1010_1010 | (1 << 3));
        assert_eq!(masks.slave, 0b0101_0001);
    }

    #[test]
    fn invalid_irq_is_rejected_without_io() {
        let mut pic = MockPic::default();
        assert_eq!(
            set_irq_mask_with_io(&mut pic, 16, false),
            Err(InvalidIrq(16))
        );
        assert_eq!(acknowledge_irq_with_io(&mut pic, 16), Err(InvalidIrq(16)));
        assert!(pic.writes.is_empty());
    }

    #[test]
    fn slave_irq_sends_slave_then_master_eoi() {
        let mut pic = MockPic::default();
        assert_eq!(
            acknowledge_irq_with_io(&mut pic, 10),
            Ok(IrqAcknowledge::Acknowledged)
        );
        assert_eq!(pic.writes, [(SLAVE_COMMAND, EOI), (MASTER_COMMAND, EOI)]);
    }

    #[test]
    fn spurious_irq7_does_not_send_eoi() {
        let mut pic = MockPic::default();
        assert_eq!(
            acknowledge_irq_with_io(&mut pic, 7),
            Ok(IrqAcknowledge::Spurious)
        );
        assert_eq!(pic.writes, [(MASTER_COMMAND, OCW3_READ_ISR)]);
    }

    #[test]
    fn spurious_irq15_sends_only_master_cascade_eoi() {
        let mut pic = MockPic::default();
        assert_eq!(
            acknowledge_irq_with_io(&mut pic, 15),
            Ok(IrqAcknowledge::Spurious)
        );
        assert_eq!(
            pic.writes,
            [(SLAVE_COMMAND, OCW3_READ_ISR), (MASTER_COMMAND, EOI)]
        );
    }
}
