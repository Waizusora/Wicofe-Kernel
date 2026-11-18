//! Architecture-independent monotonic time source contracts.
//!
//! A clock reading represents a counter value and its exact rational tick rate.
//! It deliberately does not define wall-clock time, global source selection,
//! interrupt policy, or scheduler integration.

/// A snapshot from a monotonic hardware or virtual clock.
///
/// The tick frequency is represented as frequency_numerator /
/// frequency_denominator ticks per second so sources such as the PIT retain
/// their exact input clock and divisor without rounding to integer hertz.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockReading {
    ticks: u64,
    frequency_numerator: u64,
    frequency_denominator: u64,
}

impl ClockReading {
    /// Construct a reading. A zero rate component makes the reading invalid.
    pub const fn new(ticks: u64, frequency_numerator: u64, frequency_denominator: u64) -> Option<Self> {
        if frequency_numerator == 0 || frequency_denominator == 0 {
            return None;
        }
        Some(Self { ticks, frequency_numerator, frequency_denominator })
    }

    /// Return the raw monotonic counter value.
    pub const fn ticks(self) -> u64 { self.ticks }

    /// Convert elapsed ticks to the requested units per second.
    ///
    /// Conversion uses the exact rational clock rate, a u128 intermediate,
    /// and saturating u64 output. Resolution remains limited by the source.
    pub fn elapsed_units(self, units_per_second: u64) -> Option<u64> {
        if units_per_second == 0 { return None; }
        let numerator = self.ticks as u128
            * self.frequency_denominator as u128
            * units_per_second as u128;
        let units = numerator / self.frequency_numerator as u128;
        Some(units.min(u64::MAX as u128) as u64)
    }

    /// Return elapsed nanoseconds, quantized to the source's resolution.
    pub fn elapsed_nanos(self) -> u64 {
        self.elapsed_units(1_000_000_000).expect("nanoseconds per second is non-zero")
    }
}

/// Read-only interface implemented by monotonic clock backends.
pub trait ClockSource {
    /// Return a coherent counter/rate snapshot, or None before readiness.
    fn read(&self) -> Option<ClockReading>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockClock { reading: Option<ClockReading> }

    impl ClockSource for MockClock {
        fn read(&self) -> Option<ClockReading> { self.reading }
    }

    #[test]
    fn rejects_invalid_clock_rates_and_unit_scales() {
        assert_eq!(ClockReading::new(1, 0, 1), None);
        assert_eq!(ClockReading::new(1, 1, 0), None);
        assert_eq!(ClockReading::new(1, 1_000, 1).unwrap().elapsed_units(0), None);
    }

    #[test]
    fn converts_rational_frequency_without_rounding_the_rate() {
        let reading = ClockReading::new(1, 1_193_182, 11_931).unwrap();
        assert_eq!(reading.elapsed_units(1_000), Some(9));
        assert_eq!(reading.elapsed_units(1_000_000), Some(9_999));
        assert_eq!(reading.elapsed_nanos(), 9_999_312);
    }

    #[test]
    fn source_can_report_not_ready_then_a_monotonic_snapshot() {
        let unavailable = MockClock { reading: None };
        assert_eq!(unavailable.read(), None);
        let ready = MockClock { reading: ClockReading::new(42, 100, 1) };
        assert_eq!(ready.read().unwrap().ticks(), 42);
    }

    #[test]
    fn conversion_saturates_for_large_elapsed_values() {
        let reading = ClockReading::new(u64::MAX, 1, u64::MAX).unwrap();
        assert_eq!(reading.elapsed_nanos(), u64::MAX);
    }
}
