use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use tracing::info;

/// List all available audio input and output devices
pub fn list_devices() -> Result<()> {
    let host = cpal::default_host();

    info!("Available input devices:");
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            info!("  - {}", name);
        }
    }

    info!("Available output devices:");
    for device in host.output_devices()? {
        if let Ok(name) = device.name() {
            info!("  - {}", name);
        }
    }

    Ok(())
}
