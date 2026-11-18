//! Native Wicofe Driver Framework (WDF) foundations.
//!
//! This module currently defines allocation-free, hardware-independent driver
//! lifecycle contracts. It does not enumerate hardware or access MMIO, IRQ,
//! DMA, or IOMMU resources. Those operations must be added only through the
//! corresponding kernel resource managers.
//!
//! This is an in-kernel Rust interface, not a stable DKM or userspace ABI.

/// Stable kernel-local identifier for a discovered device.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct DeviceId(u64);

impl DeviceId {
    /// Construct an identifier supplied by the kernel device registry.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Return the opaque numeric identifier.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Stable kernel-local identifier for a registered driver.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct DriverId(u64);

impl DriverId {
    /// Construct an identifier supplied by the kernel driver registry.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Return the opaque numeric identifier.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Coarse lifecycle state for one device/driver binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceState {
    /// Device discovery produced a validated device record.
    Discovered,
    /// A candidate driver has been selected.
    Matched,
    /// The candidate driver accepted the device.
    Probed,
    /// Driver initialization completed.
    Initialized,
    /// Device operations are published and available.
    Running,
    /// New operations are blocked and active work is quiesced.
    Stopped,
    /// Driver registrations and resources have been released.
    Removed,
    /// An operation failed; rollback is required before retry/removal.
    Failed,
}

/// Invalid device lifecycle transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleError {
    /// The requested transition is not valid from the current state.
    InvalidTransition {
        /// State before the attempted transition.
        from: DeviceState,
        /// Requested destination state.
        to: DeviceState,
    },
}

/// Validate and apply one lifecycle transition.
///
/// This function only updates the caller-owned state value. It does not call
/// driver callbacks or acquire/release hardware resources.
pub fn transition(
    state: &mut DeviceState,
    next: DeviceState,
) -> Result<(), LifecycleError> {
    use DeviceState::*;

    let valid = matches!(
        (*state, next),
        (Discovered, Matched)
            | (Matched, Probed)
            | (Probed, Initialized)
            | (Initialized, Running)
            | (Running, Stopped)
            | (Stopped, Running)
            | (Stopped, Removed)
            | (Discovered, Failed)
            | (Matched, Failed)
            | (Probed, Failed)
            | (Initialized, Failed)
            | (Running, Failed)
            | (Stopped, Failed)
            | (Failed, Discovered)
            | (Failed, Removed)
    );

    if !valid {
        return Err(LifecycleError::InvalidTransition {
            from: *state,
            to: next,
        });
    }

    *state = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_can_follow_the_normal_lifecycle() {
        let mut state = DeviceState::Discovered;

        for next in [
            DeviceState::Matched,
            DeviceState::Probed,
            DeviceState::Initialized,
            DeviceState::Running,
            DeviceState::Stopped,
            DeviceState::Running,
            DeviceState::Stopped,
            DeviceState::Removed,
        ] {
            assert_eq!(transition(&mut state, next), Ok(()));
            assert_eq!(state, next);
        }
    }

    #[test]
    fn invalid_transition_does_not_mutate_state() {
        let mut state = DeviceState::Discovered;

        assert_eq!(
            transition(&mut state, DeviceState::Running),
            Err(LifecycleError::InvalidTransition {
                from: DeviceState::Discovered,
                to: DeviceState::Running,
            })
        );
        assert_eq!(state, DeviceState::Discovered);
    }

    #[test]
    fn failed_device_requires_explicit_recovery_or_removal() {
        let mut state = DeviceState::Probed;
        assert_eq!(transition(&mut state, DeviceState::Failed), Ok(()));
        assert_eq!(transition(&mut state, DeviceState::Removed), Ok(()));
        assert_eq!(state, DeviceState::Removed);
    }

    #[test]
    fn removed_device_is_terminal() {
        let mut state = DeviceState::Stopped;
        assert_eq!(transition(&mut state, DeviceState::Removed), Ok(()));
        assert!(transition(&mut state, DeviceState::Running).is_err());
        assert_eq!(state, DeviceState::Removed);
    }

    #[test]
    fn identifiers_preserve_opaque_values() {
        assert_eq!(DeviceId::new(42).raw(), 42);
        assert_eq!(DriverId::new(7).raw(), 7);
    }
}
