//! Programmable Interval Timer (8253/8254) channel 0 support.
//!
//! The driver only programs the PIT. IRQ registration, PIC unmasking, and
//! CPU interrupt enabling remain explicit operations owned by the caller.

#[cfg(target_os = "none")]
use core::arch::asm;

pub const CHANNEL0_DATA: u16 = 0x40;
pub const CHANNEL2_DATA: u16 = 0x42;
pub const COMMAND: u16 = 0x43;
const SPEAKER_CONTROL: u16 = 0x61;
const CHANNEL2_ONESHOT: u8 = 0xB0;
const CHANNEL2_GATE: u8 = 1 << 0;
const SPEAKER_ENABLE: u8 = 1 << 1;
const CHANNEL2_OUTPUT: u8 = 1 << 5;
pub const INPUT_FREQUENCY_HZ: u32 = 1_193_182;
const CHANNEL0_SQUARE_WAVE: u8 = 0x36;

pub trait PitIo {
    fn write_u8(&mut self, port: u16, value: u8);
}

/// I/O required to operate PIT channel 2 through the PC speaker control port.
pub trait PitChannel2Io: PitIo {
    fn read_u8(&mut self, port: u16) -> u8;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PitError {
    ZeroFrequency,
    FrequencyOutOfRange,
    ZeroCount,
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
pub fn program_periodic_with_io<I: PitIo>(io: &mut I, frequency_hz: u32) -> Result<u16, PitError> {
    let divisor = divisor_for_frequency(frequency_hz)?;
    io.write_u8(COMMAND, CHANNEL0_SQUARE_WAVE);
    io.write_u8(CHANNEL0_DATA, divisor as u8);
    io.write_u8(CHANNEL0_DATA, (divisor >> 8) as u8);
    Ok(divisor)
}

/// Configure channel 2 as a one-shot down-counter and sample a counter
/// immediately before the PIT count bytes start the interval.
///
/// The callback runs after the mode command and immediately before the low
/// count byte. The PIT begins counting when the high byte is written.
pub fn program_channel2_oneshot_with_sample<I, F>(
    io: &mut I,
    count: u16,
    sample_start: F,
) -> Result<(u8, u64), PitError>
where
    I: PitChannel2Io,
    F: FnOnce() -> u64,
{
    if count == 0 {
        return Err(PitError::ZeroCount);
    }

    let previous_control = io.read_u8(SPEAKER_CONTROL);
    // Disable speaker output and set channel 2 gate high, preserving other bits.
    io.write_u8(
        SPEAKER_CONTROL,
        (previous_control & !SPEAKER_ENABLE) | CHANNEL2_GATE,
    );
    io.write_u8(COMMAND, CHANNEL2_ONESHOT);
    io.write_u8(CHANNEL2_DATA, count as u8);
    // The counter starts when the high byte is written; sample immediately
    // before that final write to minimize systematic interval bias.
    let start = sample_start();
    io.write_u8(CHANNEL2_DATA, (count >> 8) as u8);
    Ok((previous_control, start))
}

/// Configure channel 2 and return its previous speaker-control byte.
pub fn program_channel2_oneshot_with_io<I: PitChannel2Io>(
    io: &mut I,
    count: u16,
) -> Result<u8, PitError> {
    program_channel2_oneshot_with_sample(io, count, || 0).map(|(previous, _)| previous)
}

/// Return whether channel 2 has reached its one-shot terminal count.
pub fn channel2_oneshot_expired_with_io<I: PitChannel2Io>(io: &mut I) -> bool {
    io.read_u8(SPEAKER_CONTROL) & CHANNEL2_OUTPUT != 0
}

/// Restore the speaker-control byte captured before channel 2 was configured.
pub fn restore_channel2_control_with_io<I: PitChannel2Io>(io: &mut I, previous: u8) {
    io.write_u8(SPEAKER_CONTROL, previous);
}

#[cfg(target_os = "none")]
pub(super) struct X86PitIo;

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

#[cfg(target_os = "none")]
impl PitChannel2Io for X86PitIo {
    fn read_u8(&mut self, port: u16) -> u8 {
        let value: u8;
        // SAFETY: Caller is ring 0 and owns the PIT channel 2 operation.
        unsafe {
            asm!(
                "in al, dx",
                in("dx") port,
                out("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
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
        reads: Vec<(u16, u8)>,
        control: u8,
    }

    impl PitIo for MockPit {
        fn write_u8(&mut self, port: u16, value: u8) {
            self.writes.push((port, value));
            if port == SPEAKER_CONTROL {
                self.control = value;
            }
        }
    }

    impl PitChannel2Io for MockPit {
        fn read_u8(&mut self, port: u16) -> u8 {
            let value = if port == SPEAKER_CONTROL {
                self.control
            } else {
                0
            };
            self.reads.push((port, value));
            value
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
        assert_eq!(divisor_for_frequency(1), Err(PitError::FrequencyOutOfRange));
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

    #[test]
    fn channel2_oneshot_preserves_and_restores_speaker_control() {
        let mut pit = MockPit {
            control: 0b1100_0010,
            ..MockPit::default()
        };
        let previous = program_channel2_oneshot_with_io(&mut pit, 0x1234).unwrap();
        assert_eq!(previous, 0b1100_0010);
        assert_eq!(
            pit.writes,
            [
                (SPEAKER_CONTROL, 0b1100_0001),
                (COMMAND, CHANNEL2_ONESHOT),
                (CHANNEL2_DATA, 0x34),
                (CHANNEL2_DATA, 0x12),
            ]
        );
        assert!(!channel2_oneshot_expired_with_io(&mut pit));
        pit.control |= CHANNEL2_OUTPUT;
        assert!(channel2_oneshot_expired_with_io(&mut pit));
        restore_channel2_control_with_io(&mut pit, previous);
        assert_eq!(pit.control, previous);
    }

    #[test]
    fn channel2_oneshot_rejects_zero_count_without_io() {
        let mut pit = MockPit::default();
        assert_eq!(
            program_channel2_oneshot_with_io(&mut pit, 0),
            Err(PitError::ZeroCount)
        );
        assert!(pit.writes.is_empty());
        assert!(pit.reads.is_empty());
    }
}
