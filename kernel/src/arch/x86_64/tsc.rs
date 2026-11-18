//! x86-64 Time Stamp Counter (TSC) discovery and raw access.
//!
//! This module deliberately does not implement ClockSource. A TSC frequency
//! must be calibrated or obtained from a trusted platform source before its
//! counter can be converted into elapsed time. Invariant-TSC support alone
//! does not provide that frequency.

use core::arch::x86_64::__cpuid;

const CPUID_TSC: u32 = 1 << 4;
const CPUID_RDTSCP: u32 = 1 << 27;
const CPUID_INVARIANT_TSC: u32 = 1 << 8;

/// CPU-reported TSC capabilities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TscCapabilities {
    /// CPUID.01H:EDX.TSC.
    pub supported: bool,
    /// CPUID.80000007H:EDX.InvariantTSC.
    pub invariant: bool,
    /// CPUID.80000001H:EDX.RDTSCP.
    pub rdtscp: bool,
}

impl TscCapabilities {
    /// Decode the relevant CPUID feature bits.
    ///
    /// The maximum extended CPUID leaf must be checked by the caller before
    /// interpreting extended feature words.
    pub const fn from_cpuid(
        basic_edx: u32,
        max_extended_leaf: u32,
        extended_feature_edx: u32,
        invariant_feature_edx: u32,
    ) -> Self {
        Self {
            supported: basic_edx & CPUID_TSC != 0,
            invariant: basic_edx & CPUID_TSC != 0
                && max_extended_leaf >= 0x8000_0007
                && invariant_feature_edx & CPUID_INVARIANT_TSC != 0,
            rdtscp: basic_edx & CPUID_TSC != 0
                && max_extended_leaf >= 0x8000_0001
                && extended_feature_edx & CPUID_RDTSCP != 0,
        }
    }
}

/// Query TSC, invariant-TSC, and RDTSCP support using CPUID.
pub fn capabilities() -> TscCapabilities {
    // SAFETY: CPUID is available on all x86-64 processors.
    let basic = unsafe { __cpuid(1) };
    // SAFETY: CPUID leaf 0x8000_0000 is a standard extended-leaf query.
    let max_extended = unsafe { __cpuid(0x8000_0000) }.eax;

    let extended_features = if max_extended >= 0x8000_0001 {
        // SAFETY: The maximum extended leaf confirms this leaf exists.
        unsafe { __cpuid(0x8000_0001) }.edx
    } else {
        0
    };

    let invariant_features = if max_extended >= 0x8000_0007 {
        // SAFETY: The maximum extended leaf confirms this leaf exists.
        unsafe { __cpuid(0x8000_0007) }.edx
    } else {
        0
    };

    TscCapabilities::from_cpuid(
        basic.edx,
        max_extended,
        extended_features,
        invariant_features,
    )
}

/// Read the raw TSC counter with an LFENCE before RDTSC.
///
/// This returns an unscaled hardware counter, not nanoseconds or another time
/// unit. It is not a ClockSource until a separate calibration/selection policy
/// establishes a trustworthy frequency.
///
/// # Safety
/// Must execute in kernel context where RDTSC is permitted (CR4.TSD clear).
/// The caller must first confirm `capabilities().supported`.
#[cfg(target_os = "none")]
pub unsafe fn read_counter() -> Option<u64> {
    if !capabilities().supported {
        return None;
    }

    let low: u32;
    let high: u32;
    // SAFETY: The caller guarantees TSC availability and permission. LFENCE
    // orders prior loads before the timestamp read; it does not calibrate it.
    unsafe {
        core::arch::asm!(
            "lfence",
            "rdtsc",
            lateout("eax") low,
            lateout("edx") high,
            options(nomem, nostack)
        );
    }
    Some(((high as u64) << 32) | low as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_all_tsc_capabilities() {
        let caps = TscCapabilities::from_cpuid(
            CPUID_TSC,
            0x8000_0007,
            CPUID_RDTSCP,
            CPUID_INVARIANT_TSC,
        );
        assert_eq!(
            caps,
            TscCapabilities {
                supported: true,
                invariant: true,
                rdtscp: true,
            }
        );
    }

    #[test]
    fn extended_features_require_supported_cpuid_leaves() {
        let caps = TscCapabilities::from_cpuid(
            CPUID_TSC,
            0x8000_0000,
            CPUID_RDTSCP,
            CPUID_INVARIANT_TSC,
        );
        assert!(caps.supported);
        assert!(!caps.invariant);
        assert!(!caps.rdtscp);
    }

    #[test]
    fn tsc_may_be_absent_even_when_extended_bits_are_set() {
        let caps = TscCapabilities::from_cpuid(
            0,
            0x8000_0007,
            CPUID_RDTSCP,
            CPUID_INVARIANT_TSC,
        );
        assert!(!caps.supported);
        assert!(!caps.invariant);
        assert!(!caps.rdtscp);
    }
}
