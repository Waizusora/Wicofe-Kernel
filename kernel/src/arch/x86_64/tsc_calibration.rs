//! Pure TSC frequency calibration against PIT channel 2 intervals.
//!
//! This module deliberately contains no port I/O. Hardware sampling, timeout
//! handling, and restoration of PIT state belong to a separate boot-time layer.

use super::pit::INPUT_FREQUENCY_HZ;
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
        assert!((2_399_000_000..=2_403_000_000)
            .contains(&result.frequency.numerator_hz()));
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
