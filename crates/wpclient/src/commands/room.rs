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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_room_unreachable() {
        let mut config = Configuration::default();
        config.server.address = "127.0.0.1:1".into();
        let keypair = UserKeypair::generate();

        let res = execute_room(&config, "test_room_name".into(), keypair).await;
        assert!(res.is_err());
    }
}
