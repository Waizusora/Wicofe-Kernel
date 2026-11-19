//! Pure TSC frequency calibration against PIT channel 2 intervals.
//!
//! The estimator is pure and allocation-free. Hardware sampling is isolated
//! behind a bounded, boot-time adapter so it can be tested with mock port I/O.

use super::pit::{
    INPUT_FREQUENCY_HZ, PitChannel2Io, PitError, channel2_oneshot_expired_with_io,
    program_channel2_oneshot_with_io, restore_channel2_control_with_io,
};
use super::tsc::TscFrequency;

/// One measured interval. `pit_ticks` is the number of PIT input-clock cycles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalibrationSample {
    pub tsc_ticks: u64,
    pub pit_ticks: u32,
}

/// Maximum number of samples accepted by the allocation-free median routine.
pub const MAX_CALIBRATION_SAMPLES: usize = 9;
/// Minimum sample count required to reject isolated timing outliers.
pub const MIN_CALIBRATION_SAMPLES: usize = 3;
const PPM_SCALE: u128 = 1_000_000;

/// Result of a validated multi-sample calibration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalibrationResult {
    pub frequency: TscFrequency,
    pub spread_ppm: u32,
    pub samples_used: u8,
}

/// Reasons a calibration batch cannot be accepted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalibrationError {
    TooFewSamples,
    TooManySamples,
    ZeroDelta,
    ArithmeticOverflow,
    SpreadExceeded,
    ZeroPollBudget,
    Timeout,
    Pit(PitError),
    TscUnsupported,
}

/// Measure one PIT channel-2 interval with a strictly bounded polling loop.
///
/// The caller must establish TSC support on the current logical processor and
/// own PIT channel 2. The previous speaker-control byte is restored on both
/// completion and timeout.
pub fn measure_sample_with_io<I, F>(
    io: &mut I,
    count: u16,
    max_polls: u32,
    mut read_tsc: F,
) -> Result<CalibrationSample, CalibrationError>
where
    I: PitChannel2Io,
    F: FnMut() -> u64,
{
    if max_polls == 0 {
        return Err(CalibrationError::ZeroPollBudget);
    }

    let previous = program_channel2_oneshot_with_io(io, count).map_err(CalibrationError::Pit)?;
    let start = read_tsc();
    let mut completed = false;

    for _ in 0..max_polls {
        if channel2_oneshot_expired_with_io(io) {
            completed = true;
            break;
        }
    }

    let end = if completed { Some(read_tsc()) } else { None };
    restore_channel2_control_with_io(io, previous);

    let end = end.ok_or(CalibrationError::Timeout)?;
    let tsc_ticks = end.wrapping_sub(start);
    if tsc_ticks == 0 {
        return Err(CalibrationError::ZeroDelta);
    }

    Ok(CalibrationSample {
        tsc_ticks,
        pit_ticks: count as u32,
    })
}

/// Calibrate the local processor's TSC using repeated PIT channel-2 intervals.
///
/// # Safety
/// Must run at ring 0 with exclusive ownership of PIT channel 2 and port 0x61.
/// The caller must prevent concurrent code from changing the PC speaker/PIT
/// channel-2 state. The function does not disable interrupts; a preemption
/// can cause a timeout or an outlier, which the bounded sampler rejects.
#[cfg(target_os = "none")]
pub unsafe fn calibrate_hardware(
    sample_count: usize,
    pit_count: u16,
    max_polls: u32,
    max_spread_ppm: u32,
) -> Result<CalibrationResult, CalibrationError> {
    if sample_count < MIN_CALIBRATION_SAMPLES {
        return Err(CalibrationError::TooFewSamples);
    }
    if sample_count > MAX_CALIBRATION_SAMPLES {
        return Err(CalibrationError::TooManySamples);
    }
    if !super::tsc::detect().supported {
        return Err(CalibrationError::TscUnsupported);
    }

    let mut samples = [CalibrationSample {
        tsc_ticks: 0,
        pit_ticks: 0,
    }; MAX_CALIBRATION_SAMPLES];
    let mut io = super::pit::X86PitIo;

    for sample in samples.iter_mut().take(sample_count) {
        // SAFETY: TSC support was checked on this logical processor above.
        let measured = measure_sample_with_io(&mut io, pit_count, max_polls, || unsafe {
            super::tsc::read_unchecked()
        })?;
        *sample = measured;
    }

    calibrate(&samples[..sample_count], max_spread_ppm)
}

/// Convert a batch of TSC/PIT intervals into a median frequency estimate.
///
/// Each sample is converted using the PIT input frequency. The median limits
/// the influence of a single delayed poll or preemption. The batch is rejected
/// when its peak-to-peak spread exceeds `max_spread_ppm` relative to the
/// median estimate. At least three and at most nine samples are accepted.
///
/// This function does not establish TSC invariance, monotonicity, or
/// cross-processor synchronization; callers must validate those separately.
pub fn calibrate(
    samples: &[CalibrationSample],
    max_spread_ppm: u32,
) -> Result<CalibrationResult, CalibrationError> {
    if samples.len() < MIN_CALIBRATION_SAMPLES {
        return Err(CalibrationError::TooFewSamples);
    }
    if samples.len() > MAX_CALIBRATION_SAMPLES {
        return Err(CalibrationError::TooManySamples);
    }

    let mut estimates = [0_u64; MAX_CALIBRATION_SAMPLES];
    for (index, sample) in samples.iter().enumerate() {
        if sample.tsc_ticks == 0 || sample.pit_ticks == 0 {
            return Err(CalibrationError::ZeroDelta);
        }

        let numerator = (sample.tsc_ticks as u128)
            .checked_mul(INPUT_FREQUENCY_HZ as u128)
            .ok_or(CalibrationError::ArithmeticOverflow)?;
        let denominator = sample.pit_ticks as u128;
        let rounded_hz = numerator
            .checked_add(denominator / 2)
            .ok_or(CalibrationError::ArithmeticOverflow)?
            / denominator;
        estimates[index] =
            u64::try_from(rounded_hz).map_err(|_| CalibrationError::ArithmeticOverflow)?;
        if estimates[index] == 0 {
            return Err(CalibrationError::ZeroDelta);
        }
    }

    // Insertion sort is bounded by MAX_CALIBRATION_SAMPLES and needs no alloc.
    for index in 1..samples.len() {
        let value = estimates[index];
        let mut position = index;
        while position > 0 && estimates[position - 1] > value {
            estimates[position] = estimates[position - 1];
            position -= 1;
        }
        estimates[position] = value;
    }

    let median = estimates[samples.len() / 2];
    let minimum = estimates[0];
    let maximum = estimates[samples.len() - 1];
    let spread = (maximum - minimum) as u128;
    let spread_ppm = spread
        .checked_mul(PPM_SCALE)
        .ok_or(CalibrationError::ArithmeticOverflow)?
        / median as u128;
    let spread_ppm = u32::try_from(spread_ppm).map_err(|_| CalibrationError::ArithmeticOverflow)?;

    if spread_ppm > max_spread_ppm {
        return Err(CalibrationError::SpreadExceeded);
    }

    let frequency = TscFrequency::new(median, 1).ok_or(CalibrationError::ArithmeticOverflow)?;
    Ok(CalibrationResult {
        frequency,
        spread_ppm,
        samples_used: samples.len() as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_for_hz(hz: u64) -> CalibrationSample {
        // Use a 10 ms PIT interval, exactly 11,931 PIT input cycles.
        let pit_ticks = 11_931_u32;
        let tsc_ticks = ((hz as u128 * pit_ticks as u128) / INPUT_FREQUENCY_HZ as u128) as u64;
        CalibrationSample {
            tsc_ticks,
            pit_ticks,
        }
    }

    struct MockPit {
        control: u8,
        polls_until_expiry: Option<u32>,
        polls: u32,
        writes: std::vec::Vec<(u16, u8)>,
    }

    impl super::super::pit::PitIo for MockPit {
        fn write_u8(&mut self, port: u16, value: u8) {
            self.writes.push((port, value));
            if port == 0x61 {
                self.control = value;
            }
        }
    }

    impl PitChannel2Io for MockPit {
        fn read_u8(&mut self, port: u16) -> u8 {
            if port == 0x61 {
                self.polls += 1;
                if self
                    .polls_until_expiry
                    .is_some_and(|limit| self.polls > limit)
                {
                    self.control |= 1 << 5;
                }
                self.control
            } else {
                0
            }
        }
    }

    #[test]
    fn bounded_sampler_returns_delta_and_restores_control() {
        let mut pit = MockPit {
            control: 0b1000_0010,
            polls_until_expiry: Some(2),
            polls: 0,
            writes: std::vec::Vec::new(),
        };
        let mut reads = [100_u64, 1_000_u64].into_iter();
        let sample = measure_sample_with_io(&mut pit, 10_000, 8, || reads.next().unwrap()).unwrap();
        assert_eq!(sample.tsc_ticks, 900);
        assert_eq!(sample.pit_ticks, 10_000);
        assert_eq!(pit.control, 0b1000_0010);
        assert_eq!(pit.writes.last(), Some(&(0x61, 0b1000_0010)));
    }

    #[test]
    fn bounded_sampler_timeout_restores_control() {
        let mut pit = MockPit {
            control: 0b0100_0010,
            polls_until_expiry: None,
            polls: 0,
            writes: std::vec::Vec::new(),
        };
        assert_eq!(
            measure_sample_with_io(&mut pit, 10_000, 3, || 100),
            Err(CalibrationError::Timeout)
        );
        // One initial control-byte snapshot plus exactly three bounded polls.
        assert_eq!(pit.polls, 4);
        assert_eq!(pit.control, 0b0100_0010);
        assert_eq!(pit.writes.last(), Some(&(0x61, 0b0100_0010)));
    }

    #[test]
    fn bounded_sampler_rejects_zero_budget_without_io() {
        let mut pit = MockPit {
            control: 0x02,
            polls_until_expiry: None,
            polls: 0,
            writes: std::vec::Vec::new(),
        };
        assert_eq!(
            measure_sample_with_io(&mut pit, 10_000, 0, || 100),
            Err(CalibrationError::ZeroPollBudget)
        );
        assert!(pit.writes.is_empty());
        assert_eq!(pit.polls, 0);
    }

    #[test]
    fn computes_median_frequency_from_three_samples() {
        let samples = [
            sample_for_hz(2_400_000_000),
            sample_for_hz(2_402_000_000),
            sample_for_hz(2_401_000_000),
        ];
        let result = calibrate(&samples, 2_000).unwrap();
        assert_eq!(result.samples_used, 3);
        assert_eq!(result.frequency.denominator(), 1);
        assert!((2_399_000_000..=2_403_000_000).contains(&result.frequency.numerator_hz()));
    }

    #[test]
    fn median_limits_single_outlier_influence() {
        let samples = [
            sample_for_hz(2_400_000_000),
            sample_for_hz(2_401_000_000),
            sample_for_hz(2_402_000_000),
            sample_for_hz(2_401_000_000),
            sample_for_hz(3_200_000_000),
        ];
        let result = calibrate(&samples, 400_000).unwrap();
        assert!((2_399_000_000..=2_403_000_000).contains(&result.frequency.numerator_hz()));
    }

    #[test]
    fn rejects_insufficient_or_excessive_sample_counts() {
        assert_eq!(
            calibrate(&[sample_for_hz(2_400_000_000); 2], 10_000),
            Err(CalibrationError::TooFewSamples)
        );
        assert_eq!(
            calibrate(&[sample_for_hz(2_400_000_000); 10], 10_000),
            Err(CalibrationError::TooManySamples)
        );
    }

    #[test]
    fn rejects_zero_intervals() {
        let samples = [
            sample_for_hz(2_400_000_000),
            CalibrationSample {
                tsc_ticks: 0,
                pit_ticks: 1,
            },
            sample_for_hz(2_400_000_000),
        ];
        assert_eq!(
            calibrate(&samples, 10_000),
            Err(CalibrationError::ZeroDelta)
        );
    }

    #[test]
    fn rejects_unstable_sample_batches() {
        let samples = [
            sample_for_hz(2_000_000_000),
            sample_for_hz(2_400_000_000),
            sample_for_hz(2_800_000_000),
        ];
        assert_eq!(
            calibrate(&samples, 10_000),
            Err(CalibrationError::SpreadExceeded)
        );
    }

    #[test]
    fn rejects_frequency_arithmetic_overflow() {
        let samples = [CalibrationSample {
            tsc_ticks: u64::MAX,
            pit_ticks: 1,
        }; 3];
        assert_eq!(
            calibrate(&samples, u32::MAX),
            Err(CalibrationError::ArithmeticOverflow)
        );
    }
}
