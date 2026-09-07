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

/// Find an input audio device by name or substring, or return default input device if None.
pub fn find_input_device(host: &cpal::Host, name_opt: Option<&str>) -> Result<cpal::Device> {
    if let Some(name) = name_opt {
        let name_lower = name.to_lowercase();
        let devices = host.input_devices()?;
        for device in devices {
            if let Ok(dev_name) = device.name()
                && dev_name.to_lowercase().contains(&name_lower)
            {
                return Ok(device);
            }
        }
        anyhow::bail!("Input audio device containing '{}' not found", name);
    } else {
        host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No default input audio device available"))
    }
}

/// Find an output audio device by name or substring, or return default output device if None.
pub fn find_output_device(host: &cpal::Host, name_opt: Option<&str>) -> Result<cpal::Device> {
    if let Some(name) = name_opt {
        let name_lower = name.to_lowercase();
        let devices = host.output_devices()?;
        for device in devices {
            if let Ok(dev_name) = device.name()
                && dev_name.to_lowercase().contains(&name_lower)
            {
                return Ok(device);
            }
        }
        anyhow::bail!("Output audio device containing '{}' not found", name);
    } else {
        host.default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No default output audio device available"))
    }
}
