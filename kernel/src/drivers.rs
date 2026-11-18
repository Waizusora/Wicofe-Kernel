//! Native Wicofe Driver Framework (WDF) foundations.
//!
//! This module provides allocation-free, hardware-independent device identity,
//! driver matching, bounded device registration, and lifecycle contracts. It
//! does not enumerate hardware or access MMIO, IRQ, DMA, or IOMMU resources.
//! Those operations must be added only through the corresponding kernel
//! resource managers.
//!
//! The registry is deliberately unsynchronized. Until kernel-wide locking and
//! SMP-safe primitives are available, callers must serialize every mutation.
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

/// Discovery domain used to prevent cross-bus identifier collisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusType {
    /// PCI or PCI Express.
    Pci,
    /// USB device or interface.
    Usb,
    /// Firmware-described or SoC-integrated platform device.
    Platform,
    /// Kernel-created virtual device.
    Virtual,
}

/// Normalized, hardware-independent identity fields.
///
/// Fields not supplied by a bus enumerator remain `None`. PCI-specific
/// fields must not be fabricated for USB or platform devices. Future ACPI and
/// Device Tree enumerators can extend this identity with validated firmware
/// identifiers without changing the lifecycle contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceIdentity {
    /// Discovery bus.
    pub bus: BusType,
    /// Bus/vendor-assigned vendor identifier, when defined.
    pub vendor_id: Option<u16>,
    /// Bus/vendor-assigned product or device identifier, when defined.
    pub device_id: Option<u16>,
    /// Normalized device class, when defined by the bus.
    pub class_code: Option<u8>,
    /// Normalized device subclass, when defined by the bus.
    pub subclass: Option<u8>,
    /// Programming interface or function subtype, when defined.
    pub programming_interface: Option<u8>,
    /// Hardware revision, when available.
    pub revision: Option<u8>,
}

impl DeviceIdentity {
    /// Create an identity with only its discovery bus known.
    pub const fn new(bus: BusType) -> Self {
        Self {
            bus,
            vendor_id: None,
            device_id: None,
            class_code: None,
            subclass: None,
            programming_interface: None,
            revision: None,
        }
    }
}

/// Declarative driver matching rule.
///
/// Every populated field must match. A rule with no criteria beyond the bus
/// is intentionally rejected by `is_specific` so accidental catch-all
/// drivers cannot bind every device on a bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DriverMatch {
    /// Required bus, or any bus when absent.
    pub bus: Option<BusType>,
    /// Required vendor identifier.
    pub vendor_id: Option<u16>,
    /// Required device/product identifier.
    pub device_id: Option<u16>,
    /// Required class code.
    pub class_code: Option<u8>,
    /// Required subclass.
    pub subclass: Option<u8>,
    /// Required programming interface.
    pub programming_interface: Option<u8>,
    /// Required hardware revision.
    pub revision: Option<u8>,
}

impl DriverMatch {
    /// Return true when at least one identity field beyond the bus is set.
    pub const fn is_specific(self) -> bool {
        self.vendor_id.is_some()
            || self.device_id.is_some()
            || self.class_code.is_some()
            || self.subclass.is_some()
            || self.programming_interface.is_some()
            || self.revision.is_some()
    }

    /// Test an identity against all criteria in this rule.
    ///
    /// Empty rules never match. A missing identity field cannot satisfy a
    /// populated rule criterion.
    pub fn matches(self, identity: DeviceIdentity) -> bool {
        self.is_specific()
            && self.bus.is_none_or(|value| value == identity.bus)
            && self
                .vendor_id
                .is_none_or(|value| Some(value) == identity.vendor_id)
            && self
                .device_id
                .is_none_or(|value| Some(value) == identity.device_id)
            && self
                .class_code
                .is_none_or(|value| Some(value) == identity.class_code)
            && self
                .subclass
                .is_none_or(|value| Some(value) == identity.subclass)
            && self
                .programming_interface
                .is_none_or(|value| Some(value) == identity.programming_interface)
            && self
                .revision
                .is_none_or(|value| Some(value) == identity.revision)
    }
}

/// Static metadata used by the future driver binder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DriverDescriptor {
    /// Kernel-local driver identifier.
    pub id: DriverId,
    /// Diagnostic driver name.
    pub name: &'static str,
    /// Device identity rule.
    pub match_rule: DriverMatch,
}

impl DriverDescriptor {
    /// Check whether this driver declares a valid, matching rule.
    pub fn supports(self, identity: DeviceIdentity) -> bool {
        self.match_rule.matches(identity)
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
pub fn transition(state: &mut DeviceState, next: DeviceState) -> Result<(), LifecycleError> {
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

/// A registered device record. It contains no hardware pointers or resources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceRecord {
    /// Stable kernel-local identity.
    pub id: DeviceId,
    /// Validated, normalized discovery identity.
    pub identity: DeviceIdentity,
    /// Current driver lifecycle state.
    pub state: DeviceState,
}

/// Errors returned by the bounded device registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// The identifier is already registered.
    DuplicateDevice(DeviceId),
    /// No free registry slot remains.
    Full,
    /// The requested identifier is not registered.
    NotFound(DeviceId),
    /// A device must reach Removed before its record can be discarded.
    DeviceNotRemoved(DeviceId),
}

/// Fixed-capacity device registry for the allocation-free WDF foundation.
///
/// The caller chooses capacity at compile time. Exhaustion is explicit; the
/// registry never allocates, silently overwrites, or evicts a device record.
pub struct DeviceRegistry<const N: usize> {
    entries: [Option<DeviceRecord>; N],
    len: usize,
}

impl<const N: usize> DeviceRegistry<N> {
    /// Create an empty registry.
    pub const fn new() -> Self {
        Self {
            entries: [None; N],
            len: 0,
        }
    }

    /// Number of currently registered devices.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the registry contains no devices.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Maximum number of records this registry can hold.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Register a discovered device with its initial lifecycle state.
    pub fn register(
        &mut self,
        id: DeviceId,
        identity: DeviceIdentity,
    ) -> Result<(), RegistryError> {
        if self.entries.iter().flatten().any(|record| record.id == id) {
            return Err(RegistryError::DuplicateDevice(id));
        }

        let slot = self
            .entries
            .iter_mut()
            .find(|entry| entry.is_none())
            .ok_or(RegistryError::Full)?;

        *slot = Some(DeviceRecord {
            id,
            identity,
            state: DeviceState::Discovered,
        });
        self.len += 1;
        Ok(())
    }

    /// Return a copy of a registered device record.
    pub fn get(&self, id: DeviceId) -> Option<DeviceRecord> {
        self.entries
            .iter()
            .flatten()
            .find(|record| record.id == id)
            .copied()
    }

    /// Apply a validated lifecycle transition to a registered device.
    pub fn transition(
        &mut self,
        id: DeviceId,
        next: DeviceState,
    ) -> Result<(), RegistryErrorOrLifecycle> {
        let record = self
            .entries
            .iter_mut()
            .flatten()
            .find(|record| record.id == id)
            .ok_or(RegistryErrorOrLifecycle::Registry(RegistryError::NotFound(
                id,
            )))?;

        transition(&mut record.state, next).map_err(RegistryErrorOrLifecycle::Lifecycle)
    }

    /// Discard a device record only after it has reached Removed.
    pub fn unregister(&mut self, id: DeviceId) -> Result<(), RegistryError> {
        let slot = self
            .entries
            .iter_mut()
            .find(|entry| entry.is_some_and(|record| record.id == id))
            .ok_or(RegistryError::NotFound(id))?;

        if !slot.is_some_and(|record| record.state == DeviceState::Removed) {
            return Err(RegistryError::DeviceNotRemoved(id));
        }

        *slot = None;
        self.len -= 1;
        Ok(())
    }
}

impl<const N: usize> Default for DeviceRegistry<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Combined error for registry operations that also validate lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryErrorOrLifecycle {
    /// Registry lookup or capacity error.
    Registry(RegistryError),
    /// Invalid lifecycle transition.
    Lifecycle(LifecycleError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pci_identity(vendor_id: u16, device_id: u16) -> DeviceIdentity {
        DeviceIdentity {
            bus: BusType::Pci,
            vendor_id: Some(vendor_id),
            device_id: Some(device_id),
            class_code: Some(0x03),
            subclass: Some(0x00),
            programming_interface: Some(0x00),
            revision: Some(1),
        }
    }

    fn pci_gpu_rule(vendor_id: u16, device_id: u16) -> DriverMatch {
        DriverMatch {
            bus: Some(BusType::Pci),
            vendor_id: Some(vendor_id),
            device_id: Some(device_id),
            class_code: Some(0x03),
            subclass: Some(0x00),
            programming_interface: None,
            revision: None,
        }
    }

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

    #[test]
    fn driver_match_requires_all_declared_fields() {
        let rule = pci_gpu_rule(0x8086, 0x1234);
        assert!(rule.matches(pci_identity(0x8086, 0x1234)));
        assert!(!rule.matches(pci_identity(0x1002, 0x1234)));
        assert!(!rule.matches(pci_identity(0x8086, 0x5678)));

        let mut other_bus = pci_identity(0x8086, 0x1234);
        other_bus.bus = BusType::Usb;
        assert!(!rule.matches(other_bus));
    }

    #[test]
    fn empty_driver_match_rule_never_matches() {
        let rule = DriverMatch {
            bus: Some(BusType::Pci),
            vendor_id: None,
            device_id: None,
            class_code: None,
            subclass: None,
            programming_interface: None,
            revision: None,
        };
        assert!(!rule.is_specific());
        assert!(!rule.matches(pci_identity(0x8086, 0x1234)));
    }

    #[test]
    fn descriptor_exposes_declared_support_without_binding() {
        let descriptor = DriverDescriptor {
            id: DriverId::new(1),
            name: "test-intel-gpu",
            match_rule: pci_gpu_rule(0x8086, 0x1234),
        };
        assert!(descriptor.supports(pci_identity(0x8086, 0x1234)));
        assert!(!descriptor.supports(pci_identity(0x1002, 0x1234)));
    }

    #[test]
    fn registry_registers_and_looks_up_devices() {
        let mut registry = DeviceRegistry::<2>::new();
        let id = DeviceId::new(10);
        let identity = pci_identity(0x8086, 0x1234);

        assert_eq!(registry.len(), 0);
        assert_eq!(registry.register(id, identity), Ok(()));
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.get(id),
            Some(DeviceRecord {
                id,
                identity,
                state: DeviceState::Discovered,
            })
        );
    }

    #[test]
    fn registry_rejects_duplicate_ids_and_reports_capacity() {
        let mut registry = DeviceRegistry::<1>::new();
        let first = DeviceId::new(1);
        assert_eq!(
            registry.register(first, pci_identity(0x8086, 0x1234)),
            Ok(())
        );
        assert_eq!(
            registry.register(first, pci_identity(0x8086, 0x5678)),
            Err(RegistryError::DuplicateDevice(first))
        );
        assert_eq!(
            registry.register(DeviceId::new(2), pci_identity(0x1002, 0x1234)),
            Err(RegistryError::Full)
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn registry_requires_removed_state_before_unregister() {
        let mut registry = DeviceRegistry::<1>::new();
        let id = DeviceId::new(7);
        registry.register(id, pci_identity(0x8086, 0x1234)).unwrap();

        assert_eq!(
            registry.unregister(id),
            Err(RegistryError::DeviceNotRemoved(id))
        );

        for next in [
            DeviceState::Matched,
            DeviceState::Probed,
            DeviceState::Initialized,
            DeviceState::Running,
            DeviceState::Stopped,
            DeviceState::Removed,
        ] {
            registry.transition(id, next).unwrap();
        }

        assert_eq!(registry.unregister(id), Ok(()));
        assert_eq!(registry.len(), 0);
        assert_eq!(registry.get(id), None);
    }

    #[test]
    fn registry_reuses_slots_after_safe_unregister() {
        let mut registry = DeviceRegistry::<1>::new();
        let first = DeviceId::new(1);
        let second = DeviceId::new(2);
        let identity = pci_identity(0x8086, 0x1234);

        registry.register(first, identity).unwrap();
        registry.transition(first, DeviceState::Removed).unwrap();
        registry.unregister(first).unwrap();
        assert_eq!(registry.register(second, identity), Ok(()));
        assert_eq!(registry.get(second).unwrap().state, DeviceState::Discovered);
    }
}
