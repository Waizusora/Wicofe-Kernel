//! Programmable Interval Timer (8253/8254) channel 0 support.
//!
//! The driver only programs the PIT. IRQ registration, PIC unmasking, and
//! CPU interrupt enabling remain explicit operations owned by the caller.

#[cfg(target_os = "none")]
use core::arch::asm;

pub const CHANNEL0_DATA: u16 = 0x40;
pub const COMMAND: u16 = 0x43;
pub const INPUT_FREQUENCY_HZ: u32 = 1_193_182;
const CHANNEL0_SQUARE_WAVE: u8 = 0x36;

pub trait PitIo {
    fn write_u8(&mut self, port: u16, value: u8);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PitError {
    ZeroFrequency,
    FrequencyOutOfRange,
}

/// Convert a requested tick frequency to the 16-bit PIT divisor.
pub fn divisor_for_frequency(frequency_hz: u32) -> Result<u16, PitError> {
    if frequency_hz == 0 {
        return Err(PitError::ZeroFrequency);
    }
    let divisor = INPUT_FREQUENCY_HZ / frequency_hz;
    if divisor == 0 || divisor > u16::MAX as u32 {
        return Err(PitError::FrequencyOutOfRange);
    }
    Ok(divisor as u16)
}

/// Program channel 0 in periodic square-wave mode.
pub fn program_periodic_with_io<I: PitIo>(
    io: &mut I,
    frequency_hz: u32,
) -> Result<u16, PitError> {
    let divisor = divisor_for_frequency(frequency_hz)?;
    io.write_u8(COMMAND, CHANNEL0_SQUARE_WAVE);
    io.write_u8(CHANNEL0_DATA, divisor as u8);
    io.write_u8(CHANNEL0_DATA, (divisor >> 8) as u8);
    Ok(divisor)
}

#[cfg(target_os = "none")]
struct X86PitIo;

#[cfg(target_os = "none")]
impl PitIo for X86PitIo {
    fn write_u8(&mut self, port: u16, value: u8) {
        // SAFETY: Caller is ring 0 and has exclusive ownership of PIT setup.
        unsafe {
            asm!(
                "out dx, al",
                in("dx") port,
                in("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

/// Program PIT channel 0 at a periodic frequency.
///
/// # Safety
/// Must execute at ring 0 with exclusive ownership of PIT programming. The
/// caller must install an IRQ0 handler before unmasking IRQ0 or executing STI.
#[cfg(target_os = "none")]
pub unsafe fn initialize_periodic(frequency_hz: u32) -> Result<u16, PitError> {
    program_periodic_with_io(&mut X86PitIo, frequency_hz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    #[derive(Default)]
    struct MockPit {
        writes: Vec<(u16, u8)>,
    }

    impl PitIo for MockPit {
        fn write_u8(&mut self, port: u16, value: u8) {
            self.writes.push((port, value));
        }
    }

    #[test]
    fn computes_common_tick_rates() {
        assert_eq!(divisor_for_frequency(100), Ok(11_931));
        assert_eq!(divisor_for_frequency(1_000), Ok(1_193));
    }

    #[test]
    fn rejects_zero_and_out_of_range_frequencies() {
        assert_eq!(divisor_for_frequency(0), Err(PitError::ZeroFrequency));
        assert_eq!(
            divisor_for_frequency(1),
            Err(PitError::FrequencyOutOfRange)
        );
        assert_eq!(
            divisor_for_frequency(INPUT_FREQUENCY_HZ + 1),
            Err(PitError::FrequencyOutOfRange)
        );
    }

    #[test]
    fn programs_command_then_divisor_low_byte_and_high_byte() {
        let mut pit = MockPit::default();
        assert_eq!(program_periodic_with_io(&mut pit, 100), Ok(11_931));
        assert_eq!(
            pit.writes,
            [
                (COMMAND, CHANNEL0_SQUARE_WAVE),
                (CHANNEL0_DATA, 11_931_u16 as u8),
                (CHANNEL0_DATA, (11_931_u16 >> 8) as u8),
            ]
        );
    }
}
