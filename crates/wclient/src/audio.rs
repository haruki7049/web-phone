//! Audio device enumeration module.
//!
//! This module provides functionality for listing available audio
//! input (microphone) and output (speaker) devices on the system.

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use tracing::info;

/// List all available audio input and output devices.
///
/// This function enumerates all audio devices on the system and logs
/// their names. It's useful for debugging audio issues or helping
/// users select the correct device.
///
/// # Returns
///
/// Returns `Ok(())` if devices were successfully enumerated, or an
/// error if the audio host could not be accessed.
///
/// # Example
///
/// ```ignore
/// use wclient::audio::list_devices;
///
/// list_devices()?;
/// // Output:
/// // INFO Available input devices:
/// // INFO   - Built-in Microphone
/// // INFO Available output devices:
/// // INFO   - Built-in Speakers
/// ```
pub fn list_devices() -> Result<()> {
    let host = cpal::default_host();

    info!("Available input devices:");
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            info!("  - {}", name);
            if let Ok(config) = device.default_input_config() {
                info!("      Default input config: {:?}", config);
            }
        }
    }

    info!("Available output devices:");
    for device in host.output_devices()? {
        if let Ok(name) = device.name() {
            info!("  - {}", name);
            if let Ok(config) = device.default_output_config() {
                info!("      Default output config: {:?}", config);
            }
        }
    }

    Ok(())
}
