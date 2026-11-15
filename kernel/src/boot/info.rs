//! Architecture-neutral, normalized boot information.
//!
//! These Rust types are not the Limine wire ABI and are not a stable DKM ABI.
//! A protocol adapter must validate raw responses before constructing them.

use super::validate::{validate_memory_map, MemoryMapError};

/// Kind of physical memory described by the bootloader memory map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryRegionKind {
    Usable,
    BootloaderReclaimable,
    FirmwareReclaimable,
    KernelAndModules,
    FirmwareReserved,
    Device,
    Bad,
    Unknown,
}

/// A physical memory range. The range is `base..base + length`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryRegion {
    pub base: u64,
    pub length: u64,
    pub kind: MemoryRegionKind,
    pub attributes: u64,
}

impl MemoryRegion {
    pub const fn end(self) -> Option<u64> {
        self.base.checked_add(self.length)
    }

    pub const fn is_valid(self) -> bool {
        self.length != 0 && self.end().is_some()
    }
}

/// Physical and virtual extent of the loaded kernel image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelImage {
    pub physical_base: u64,
    pub virtual_base: u64,
    pub length: u64,
}

impl KernelImage {
    pub const fn is_valid(self) -> bool {
        self.length != 0
            && self.physical_base.checked_add(self.length).is_some()
            && self.virtual_base.checked_add(self.length).is_some()
    }
}

/// A boot-loaded module. Its bytes remain reserved until explicitly released.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootModule {
    pub physical_base: u64,
    pub length: u64,
}

impl BootModule {
    pub const fn is_valid(self) -> bool {
        self.length != 0 && self.physical_base.checked_add(self.length).is_some()
    }
}

/// Optional framebuffer description in normalized physical-address form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramebufferInfo {
    pub physical_base: u64,
    pub byte_length: u64,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bits_per_pixel: u16,
}

impl FramebufferInfo {
    pub const fn is_valid(self) -> bool {
        self.byte_length != 0
            && self.width != 0
            && self.height != 0
            && self.pitch != 0
            && self.bits_per_pixel != 0
            && self.physical_base.checked_add(self.byte_length).is_some()
    }
}

/// Validated, protocol-independent boot information.
///
/// The slices must refer to memory whose lifetime is guaranteed by the caller.
/// Bootloader-reclaimable backing memory must not be released while this value
/// or any of its slices remain in use.
#[derive(Debug)]
pub struct BootInfo<'a> {
    pub memory_map: &'a [MemoryRegion],
    pub kernel_image: KernelImage,
    pub modules: &'a [BootModule],
    /// Higher-half direct-map offset, if supplied by the boot protocol.
    /// This is an offset for virtual mapping, not a physical address.
    pub hhdm_offset: Option<u64>,
    pub framebuffer: Option<FramebufferInfo>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootInfoError {
    InvalidMemoryRegion,
    OverlappingMemoryRegions,
    InvalidKernelImage,
    InvalidBootModule,
    InvalidFramebuffer,
}

impl BootInfo<'_> {
    /// Performs range validation before memory initialization.
    ///
    /// This does not prove that bootloader-provided pointers are safe to
    /// dereference. The protocol adapter must establish pointer validity and
    /// response lifetime before constructing BootInfo.
    pub fn validate(&self) -> Result<(), BootInfoError> {
        if !self.kernel_image.is_valid() {
            return Err(BootInfoError::InvalidKernelImage);
        }

        match validate_memory_map(self.memory_map) {
            Ok(()) => {}
            Err(MemoryMapError::EmptyRegion | MemoryMapError::AddressOverflow) => {
                return Err(BootInfoError::InvalidMemoryRegion);
            }
            Err(MemoryMapError::OverlappingRegions) => {
                return Err(BootInfoError::OverlappingMemoryRegions);
            }
        }

        if self.modules.iter().any(|module| !module.is_valid()) {
            return Err(BootInfoError::InvalidBootModule);
        }

        if self.framebuffer.is_some_and(|fb| !fb.is_valid()) {
            return Err(BootInfoError::InvalidFramebuffer);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_eq;

    const KERNEL: KernelImage = KernelImage {
        physical_base: 0x20_0000,
        virtual_base: 0xffff_ffff_8020_0000,
        length: 0x10_0000,
    };

    #[test]
    fn accepts_well_formed_boot_information() {
        let memory_map = [MemoryRegion {
            base: 0x10_0000,
            length: 0x40_0000,
            kind: MemoryRegionKind::Usable,
            attributes: 0,
        }];
        let modules = [BootModule {
            physical_base: 0x50_0000,
            length: 0x20_000,
        }];
        let info = BootInfo {
            memory_map: &memory_map,
            kernel_image: KERNEL,
            modules: &modules,
            hhdm_offset: Some(0xffff_8000_0000_0000),
            framebuffer: None,
        };
        assert_eq!(info.validate(), Ok(()));
    }

    #[test]
    fn rejects_zero_length_memory_region() {
        let memory_map = [MemoryRegion {
            base: 0x1000,
            length: 0,
            kind: MemoryRegionKind::Usable,
            attributes: 0,
        }];
        let info = BootInfo {
            memory_map: &memory_map,
            kernel_image: KERNEL,
            modules: &[],
            hhdm_offset: None,
            framebuffer: None,
        };
        assert_eq!(info.validate(), Err(BootInfoError::InvalidMemoryRegion));
    }

    #[test]
    fn rejects_overlapping_memory_regions() {
        let memory_map = [
            MemoryRegion {
                base: 0x1000,
                length: 0x3000,
                kind: MemoryRegionKind::Usable,
                attributes: 0,
            },
            MemoryRegion {
                base: 0x2000,
                length: 0x1000,
                kind: MemoryRegionKind::FirmwareReserved,
                attributes: 0,
            },
        ];
        let info = BootInfo {
            memory_map: &memory_map,
            kernel_image: KERNEL,
            modules: &[],
            hhdm_offset: None,
            framebuffer: None,
        };
        assert_eq!(
            info.validate(),
            Err(BootInfoError::OverlappingMemoryRegions)
        );
    }

    #[test]
    fn rejects_overflowing_kernel_range() {
        let info = BootInfo {
            memory_map: &[],
            kernel_image: KernelImage {
                physical_base: u64::MAX - 0xff,
                virtual_base: 0xffff_ffff_8020_0000,
                length: 0x100,
            },
            modules: &[],
            hhdm_offset: None,
            framebuffer: None,
        };
        assert_eq!(info.validate(), Err(BootInfoError::InvalidKernelImage));
    }

    #[test]
    fn rejects_overflowing_module_range() {
        let modules = [BootModule {
            physical_base: u64::MAX - 0x10,
            length: 0x20,
        }];
        let info = BootInfo {
            memory_map: &[],
            kernel_image: KERNEL,
            modules: &modules,
            hhdm_offset: None,
            framebuffer: None,
        };
        assert_eq!(info.validate(), Err(BootInfoError::InvalidBootModule));
    }
}
