//! Nostr identity and Bech32 encoding/decoding utilities.

use bech32::Hrp;

/// Decode Bech32 encoded string into HRP and 8-bit payload data using standard `bech32` crate.
pub fn decode_bech32(s: &str) -> Result<(String, Vec<u8>), String> {
    let (hrp, data) =
        bech32::decode(s.trim()).map_err(|e| format!("Bech32 decode error: {}", e))?;
    Ok((hrp.as_str().to_string(), data))
}

/// Encode 8-bit byte slice to Bech32 string with specified HRP prefix using standard `bech32` crate.
pub fn encode_bech32(hrp: &str, data_8bit: &[u8]) -> Result<String, String> {
    let hrp_obj = Hrp::parse(hrp).map_err(|e| format!("Invalid HRP: {}", e))?;
    bech32::encode::<bech32::Bech32>(hrp_obj, data_8bit)
        .map_err(|e| format!("Bech32 encode error: {}", e))
}

/// Parse a Nostr key (`nsec1...`, `npub1...`, or 64-character Hex string) into 32 raw bytes.
pub fn parse_nostr_key_to_bytes(input: &str) -> Result<[u8; 32], String> {
    let trimmed = input.trim();
    if trimmed.starts_with("nsec1") || trimmed.starts_with("npub1") {
        let (hrp, data_bytes) = decode_bech32(trimmed)?;
        if hrp != "nsec" && hrp != "npub" {
            return Err(format!("Unsupported Nostr HRP prefix: {}", hrp));
        }
        if data_bytes.len() != 32 {
            return Err(format!(
                "Invalid Nostr key byte length: got {} bytes, expected 32",
                data_bytes.len()
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&data_bytes);
        return Ok(arr);
    }

    if trimmed.len() == 64 {
        let mut arr = [0u8; 32];
        hex::decode_to_slice(trimmed, &mut arr)
            .map_err(|e| format!("Invalid Hex key format: {}", e))?;
        return Ok(arr);
    }

    Err("Invalid Nostr key format (expected 64-char Hex or nsec1.../npub1... Bech32)".to_string())
}
