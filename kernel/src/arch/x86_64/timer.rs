//! Monotonic tick accounting over the legacy x86-64 PIT IRQ0 source.
//!
//! Initialization registers and unmasks IRQ0 but deliberately does not execute
//! STI. The caller controls when CPU interrupts become enabled.

use core::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};

#[cfg(target_os = "none")]
use super::{irq, pit};

const STATE_UNINITIALIZED: u8 = 0;
#[cfg(target_os = "none")]
const STATE_INITIALIZING: u8 = 1;
const STATE_READY: u8 = 2;
#[cfg(target_os = "none")]
const STATE_FAILED: u8 = 3;

static STATE: AtomicU8 = AtomicU8::new(STATE_UNINITIALIZED);
static TICKS: TickCounter = TickCounter::new();
static PIT_DIVISOR: AtomicU32 = AtomicU32::new(0);

struct TickCounter {
    value: AtomicU64,
}

impl TickCounter {
    const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    fn increment(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    fn read(&self) -> u64 {
        self.value.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerError {
    AlreadyInitialized,
    Pit(super::pit::PitError),
    Irq(super::irq::IrqError),
}

#[cfg(target_os = "none")]
extern "C" fn pit_irq_handler(_: &mut irq::IrqStackFrame) {
    TICKS.increment();
}

/// Initialize PIT channel 0, register the IRQ0 handler, and unmask IRQ0.
///
/// This function does not enable CPU interrupts. The caller must explicitly
/// enable IF only after all interrupt-dependent kernel state is ready.
///
/// # Safety
/// Must execute at ring 0 after the IDT and PIC are initialized, with CPU
/// interrupts disabled and exclusive ownership of the legacy PIC/PIT.
#[cfg(target_os = "none")]
pub unsafe fn initialize_pit(frequency_hz: u32) -> Result<u16, TimerError> {
    let divisor = pit::divisor_for_frequency(frequency_hz).map_err(TimerError::Pit)?;
    STATE
        .compare_exchange(
            STATE_UNINITIALIZED,
            STATE_INITIALIZING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .map_err(|_| TimerError::AlreadyInitialized)?;

    if let Err(error) = irq::register_handler(0, pit_irq_handler) {
        STATE.store(STATE_FAILED, Ordering::Release);
        return Err(TimerError::Irq(error));
    }

    // SAFETY: The caller provides exclusive ring-0 access to the PIT.
    let programmed_divisor = match unsafe { pit::initialize_periodic(frequency_hz) } {
        Ok(value) => value,
        Err(error) => {
            STATE.store(STATE_FAILED, Ordering::Release);
            return Err(TimerError::Pit(error));
        }
    };
    PIT_DIVISOR.store(programmed_divisor as u32, Ordering::Release);

    // SAFETY: The handler is installed, the PIT is programmed, and IF is clear.
    if let Err(error) = unsafe { irq::unmask(0) } {
        PIT_DIVISOR.store(0, Ordering::Release);
        STATE.store(STATE_FAILED, Ordering::Release);
        return Err(TimerError::Irq(error));
    }

    STATE.store(STATE_READY, Ordering::Release);
    Ok(programmed_divisor)
}

/// Return whether the PIT timer has completed initialization.
pub fn is_initialized() -> bool {
    STATE.load(Ordering::Acquire) == STATE_READY
}

/// Return the number of delivered PIT ticks since initialization.
pub fn ticks() -> u64 {
    TICKS.read()
}

/// Return the integer actual PIT frequency, if initialized.
pub fn frequency_hz() -> Option<u32> {
    if !is_initialized() {
        return None;
    }
    let divisor = PIT_DIVISOR.load(Ordering::Acquire);
    Some(super::pit::INPUT_FREQUENCY_HZ / divisor)
}

/// Return tick-derived monotonic uptime in milliseconds, if initialized.
pub fn uptime_millis() -> Option<u64> {
    uptime_units(1_000)
}

/// Return tick-derived monotonic uptime in microseconds, if initialized.
pub fn uptime_micros() -> Option<u64> {
    uptime_units(1_000_000)
}

/// Return tick-derived monotonic uptime in nanoseconds, if initialized.
///
/// Values are quantized to PIT IRQ periods and saturate at `u64::MAX`.
pub fn uptime_nanos() -> Option<u64> {
    uptime_units(1_000_000_000)
}

fn uptime_units(units_per_second: u64) -> Option<u64> {
    if !is_initialized() {
        return None;
    }
    ticks_to_units(
        ticks(),
        PIT_DIVISOR.load(Ordering::Acquire),
        units_per_second,
    )
}

fn ticks_to_units(ticks: u64, divisor: u32, units_per_second: u64) -> Option<u64> {
    if divisor == 0 || units_per_second == 0 {
        return None;
    }
    let elapsed_cycles = ticks as u128 * divisor as u128;
    let units =
        (elapsed_cycles * units_per_second as u128) / super::pit::INPUT_FREQUENCY_HZ as u128;
    Some(units.min(u64::MAX as u128) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_counter_is_monotonic_and_atomic() {
        let counter = TickCounter::new();
        assert_eq!(counter.read(), 0);
        counter.increment();
        counter.increment();
        assert_eq!(counter.read(), 2);
    }

    #[test]
    fn converts_ticks_to_millisecond_microsecond_and_nanosecond_units() {
        assert_eq!(ticks_to_units(0, 11_931, 1_000), Some(0));
        assert_eq!(ticks_to_units(1, 11_931, 1_000), Some(9));
        assert_eq!(ticks_to_units(100, 11_931, 1_000), Some(999));
        assert_eq!(ticks_to_units(1, 11_931, 1_000_000), Some(9_999));
        assert_eq!(ticks_to_units(1, 11_931, 1_000_000_000), Some(9_999_312));
    }

    #[test]
    fn uptime_conversion_rejects_zero_divisor_or_unit_scale() {
        assert_eq!(ticks_to_units(10, 0, 1_000), None);
        assert_eq!(ticks_to_units(10, 11_931, 0), None);
    }

    #[test]
    fn uptime_conversion_saturates_instead_of_overflowing() {
        assert_eq!(
            ticks_to_units(u64::MAX, u16::MAX as u32, 1_000_000_000),
            Some(u64::MAX)
        );
    }
}
