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
    let room_address = UserAddress::from_room_id(&room_id);
    info!(
        "Joining room: {} (RoomAddress: {}) with address {}",
        room_id,
        room_address,
        keypair.public_key_address()
    );
    wpapi::call::start_room_call(config, room_address).await?;
    Ok(())
}
