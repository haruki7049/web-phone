//! Listing devices and registered addresses CLI command handler.

use anyhow::Result;
use wpapi::{Configuration, UserAddress};

/// List all registered peer user addresses connected to the daemon.
pub async fn list_registered_addresses(config: &Configuration) -> Result<()> {
    let endpoint = format!("{}/addresses", config.server_url());
    println!(
        "Fetching registered addresses from daemon at {}...",
        endpoint
    );
    let client = reqwest::Client::new();
    let keypair = wpapi::UserKeypair::generate();
    let (_, auth_hdr) = wpapi::build_authorization_header(&keypair, "");
    let resp = client
        .get(&endpoint)
        .header(reqwest::header::AUTHORIZATION, auth_hdr)
        .send()
        .await?;
    if resp.status().is_success() {
        let addrs: Vec<UserAddress> = resp.json().await?;
        println!("Registered User Addresses (Total: {}):", addrs.len());
        for addr in addrs {
            println!("  • Short ID: {} | Full: {}", addr.short_id(), addr);
        }
    } else {
        println!("Server returned status code: {}", resp.status());
    }
    Ok(())
}

/// List available audio input and output devices.
pub fn list_audio_devices() -> Result<()> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    println!("Audio Input Devices (Microphones):");
    if let Ok(devices) = host.input_devices() {
        for dev in devices {
            if let Ok(desc) = dev.description() {
                println!("  • {}", desc.name());
            }
        }
    }
    println!("\nAudio Output Devices (Speakers):");
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(desc) = dev.description() {
                println!("  • {}", desc.name());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_audio_devices() {
        let res = list_audio_devices();
        assert!(res.is_ok());
    }
}
