//! Windows providers. All of them run as a standard user without a driver.

pub mod amd;
pub mod devices;
pub mod gpu;
pub mod intel;
pub mod nvml;
pub mod pawnio;
pub mod pdh;
pub mod system;

use crate::SensorProvider;

/// The driver-free set, in the order the Sensors tree shows them.
pub fn providers() -> Vec<Box<dyn SensorProvider>> {
    vec![
        Box::new(system::CpuProvider::new()),
        Box::new(system::MemoryProvider),
        Box::new(gpu::GpuProvider::new()),
        Box::new(devices::StorageProvider::new()),
        Box::new(devices::BatteryProvider::default()),
        Box::new(devices::NetworkProvider::new()),
        Box::new(system::AcpiProvider::new()),
    ]
}

/// Advanced sensors through PawnIO (elevated helper only). `Err` says why not.
pub fn pawnio_providers() -> Result<Vec<Box<dyn SensorProvider>>, pawnio::PawnError> {
    // Mainboard Super I/O (LpcIO) is deliberately not probed: entering a chip's
    // configuration mode writes to ports 0x2E / 0x4E, which on many laptops belong
    // to the embedded controller. It ships only with a table of chips verified on
    // real boards (docs/hwmon-validation.md).
    match nfd_hwinfo::cpu::native().vendor.as_str() {
        "Intel" => Ok(vec![Box::new(intel::IntelMsrProvider::new(
            pawnio::PawnIo::intel_msr()?,
        ))]),
        "AMD" => {
            let id = nfd_hwinfo::cpu::native();
            if !(0x17..=0x1A).contains(&id.family) {
                return Ok(Vec::new()); // pre-Zen: not supported
            }
            Ok(vec![Box::new(amd::AmdZenProvider::new(
                pawnio::PawnIo::amd_family17()?,
            ))])
        }
        _ => Ok(Vec::new()),
    }
}
