//! SFU Group Room CLI command handler.

use anyhow::Result;
use tracing::info;
use wpapi::{Configuration, UserAddress, UserKeypair};

/// Execute SFU group room call.
pub async fn execute_room(
    config: &Configuration,
    room_id: String,
    keypair: UserKeypair,
) -> Result<()> {
    info!(
        "Joining room: {} with address {}",
        room_id,
        keypair.public_key_address()
    );
    wpapi::call::start_room_call(config, UserAddress::new(room_id)).await?;
    Ok(())
}
