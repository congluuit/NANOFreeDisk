//! Windows providers. All of them run as a standard user without a driver.

pub mod devices;
pub mod gpu;
pub mod nvml;
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
