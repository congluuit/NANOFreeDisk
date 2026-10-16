//! Windows providers. All of them run as a standard user without a driver.

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
    let vendor = nfd_hwinfo::cpu::native().vendor;
    if vendor != "Intel" {
        // AMD (AMDFamily17) and Super I/O (LpcIO) are the next slice
        return Ok(Vec::new());
    }
    let io = pawnio::PawnIo::intel_msr()?;
    Ok(vec![Box::new(intel::IntelMsrProvider::new(io))])
}
