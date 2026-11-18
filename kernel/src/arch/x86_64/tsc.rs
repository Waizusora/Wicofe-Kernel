//! x86-64 Time Stamp Counter capability discovery and access.
//!
//! Frequency data is accepted only when CPUID leaf 0x15 provides a complete,
//! non-zero ratio and nominal crystal frequency. This module does not calibrate
//! the counter, select it as a clock source, or assert cross-CPU synchronization.

use core::arch::x86_64::__cpuid_count;

/// CPUID leaf 1 EDX.TSC.
const FEATURE_TSC: u32 = 1 << 4;
/// CPUID extended leaf 0x8000_0001 EDX.RDTSCP.
const FEATURE_RDTSCP: u32 = 1 << 27;
/// CPUID extended leaf 0x8000_0007 EDX.InvariantTSC.
const FEATURE_INVARIANT_TSC: u32 = 1 << 8;

/// Rational TSC frequency in ticks per second.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TscFrequency {
    numerator_hz: u64,
    denominator: u64,
}

impl TscFrequency {
    /// Construct a frequency ratio, rejecting incomplete CPUID data.
    pub const fn new(numerator_hz: u64, denominator: u64) -> Option<Self> {
        if numerator_hz == 0 || denominator == 0 {
            return None;
        }
        Some(Self {
            numerator_hz,
            denominator,
        })
    }

    /// Numerator of the exact ticks-per-second ratio.
    pub const fn numerator_hz(self) -> u64 {
        self.numerator_hz
    }

    /// Denominator of the exact ticks-per-second ratio.
    pub const fn denominator(self) -> u64 {
        self.denominator
    }
}

/// Raw CPUID values used for deterministic capability parsing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpuidSnapshot {
    pub max_basic_leaf: u32,
    pub leaf1_edx: u32,
    pub max_extended_leaf: u32,
    pub leaf_8000_0001_edx: u32,
    pub leaf_8000_0007_edx: u32,
    pub leaf15_eax: u32,
    pub leaf15_ebx: u32,
    pub leaf15_ecx: u32,
}

/// Features and optional nominal frequency reported by CPUID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TscCapabilities {
    pub supported: bool,
    pub invariant: bool,
    pub rdtscp: bool,
    pub nominal_frequency: Option<TscFrequency>,
}

impl TscCapabilities {
    /// Interpret CPUID data without assuming that unsupported leaves are valid.
    pub const fn from_snapshot(snapshot: CpuidSnapshot) -> Self {
        let supported = snapshot.max_basic_leaf >= 1 && snapshot.leaf1_edx & FEATURE_TSC != 0;
        let invariant = snapshot.max_extended_leaf >= 0x8000_0007
            && snapshot.leaf_8000_0007_edx & FEATURE_INVARIANT_TSC != 0;
        let rdtscp = snapshot.max_extended_leaf >= 0x8000_0001
            && snapshot.leaf_8000_0001_edx & FEATURE_RDTSCP != 0;

        let nominal_frequency = if snapshot.max_basic_leaf >= 0x15 {
            let numerator = (snapshot.leaf15_ecx as u64).checked_mul(snapshot.leaf15_ebx as u64);
            match numerator {
                Some(value) => TscFrequency::new(value, snapshot.leaf15_eax as u64),
                None => None,
            }
        } else {
            None
        };

        Self {
            supported,
            invariant,
            rdtscp,
            nominal_frequency,
        }
    }

    /// Whether CPUID provides enough evidence to consider the TSC as a
    /// constant-rate candidate. Calibration and SMP checks are still required.
    pub const fn is_constant_rate_candidate(self) -> bool {
        self.supported && self.invariant && self.nominal_frequency.is_some()
    }
}

/// Query the executing logical processor's TSC capabilities.
pub fn detect() -> TscCapabilities {
    // SAFETY: CPUID is available in x86-64 long mode.
    let basic = unsafe { __cpuid_count(0, 0) };
    // SAFETY: CPUID is available in x86-64 long mode.
    let leaf1_edx = if basic.eax >= 1 {
        // SAFETY: The maximum basic leaf reports leaf 1 as supported.
        unsafe { __cpuid_count(1, 0) }.edx
    } else {
        0
    };

    // SAFETY: CPUID extended-leaf enumeration is architecturally supported.
    let extended = unsafe { __cpuid_count(0x8000_0000, 0) };
    let ext1_edx = if extended.eax >= 0x8000_0001 {
        // SAFETY: The maximum extended leaf reports this leaf as supported.
        unsafe { __cpuid_count(0x8000_0001, 0) }.edx
    } else {
        0
    };
    let ext7_edx = if extended.eax >= 0x8000_0007 {
        // SAFETY: The maximum extended leaf reports this leaf as supported.
        unsafe { __cpuid_count(0x8000_0007, 0) }.edx
    } else {
        0
    };
    let (leaf15_eax, leaf15_ebx, leaf15_ecx) = if basic.eax >= 0x15 {
        // SAFETY: The maximum basic leaf reports this leaf as supported.
        let leaf15 = unsafe { __cpuid_count(0x15, 0) };
        (leaf15.eax, leaf15.ebx, leaf15.ecx)
    } else {
        (0, 0, 0)
    };

    TscCapabilities::from_snapshot(CpuidSnapshot {
        max_basic_leaf: basic.eax,
        leaf1_edx,
        max_extended_leaf: extended.eax,
        leaf_8000_0001_edx: ext1_edx,
        leaf_8000_0007_edx: ext7_edx,
        leaf15_eax,
        leaf15_ebx,
        leaf15_ecx,
    })
}

/// Read the local logical processor's TSC.
///
/// The LFENCE/RDTSC/LFENCE sequence constrains execution around the sample.
/// It does not establish synchronization with another logical processor.
///
/// # Safety
/// The caller must first establish that CPUID reports TSC support on this
/// logical processor. Calling RDTSC without TSC support raises #UD.
#[inline]
pub unsafe fn read_unchecked() -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: RDTSC is available on x86-64. LFENCE is available in long mode.
    unsafe {
        core::arch::asm!(
            "lfence",
            "rdtsc",
            "lfence",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack)
        );
    }
    ((high as u64) << 32) | low as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_tsc_and_invariant_feature_for_candidate_status() {
        let base = CpuidSnapshot {
            max_basic_leaf: 0x15,
            leaf1_edx: FEATURE_TSC,
            max_extended_leaf: 0x8000_0007,
            leaf_8000_0007_edx: FEATURE_INVARIANT_TSC,
            leaf15_eax: 2,
            leaf15_ebx: 3,
            leaf15_ecx: 24_000_000,
            ..CpuidSnapshot::default()
        };
        let caps = TscCapabilities::from_snapshot(base);
        assert!(caps.supported);
        assert!(caps.invariant);
        assert_eq!(caps.nominal_frequency, TscFrequency::new(72_000_000, 2));
        assert!(caps.is_constant_rate_candidate());

        let mut variable = base;
        variable.leaf_8000_0007_edx = 0;
        assert!(!TscCapabilities::from_snapshot(variable).is_constant_rate_candidate());
    }

    #[test]
    fn rejects_missing_or_zero_frequency_ratio() {
        let missing_leaf = CpuidSnapshot {
            max_basic_leaf: 1,
            leaf1_edx: FEATURE_TSC,
            ..CpuidSnapshot::default()
        };
        assert_eq!(
            TscCapabilities::from_snapshot(missing_leaf).nominal_frequency,
            None
        );

        let zero_ratio = CpuidSnapshot {
            max_basic_leaf: 0x15,
            leaf15_eax: 0,
            leaf15_ebx: 3,
            leaf15_ecx: 24_000_000,
            ..CpuidSnapshot::default()
        };
        assert_eq!(
            TscCapabilities::from_snapshot(zero_ratio).nominal_frequency,
            None
        );
    }

    #[test]
    fn reports_rdtscp_only_when_extended_leaf_is_available() {
        let snapshot = CpuidSnapshot {
            max_extended_leaf: 0x8000_0001,
            leaf_8000_0001_edx: FEATURE_RDTSCP,
            ..CpuidSnapshot::default()
        };
        assert!(TscCapabilities::from_snapshot(snapshot).rdtscp);
    }
}
