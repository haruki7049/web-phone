//! 1-to-1 Call CLI command handler.

use anyhow::Result;
use tracing::info;
use wpapi::{Configuration, UserAddress, UserKeypair};

/// Execute direct 1-to-1 call or enter TUI mode.
pub async fn execute_call(
    config: &Configuration,
    target: Option<String>,
    keypair: UserKeypair,
) -> Result<()> {
    if let Some(to) = target {
        info!("Starting direct call to target: {}", to);
        wpapi::call::start_call(config, Some(UserAddress::new(to))).await?;
    } else {
        crate::tui::run_tui_with_keypair(config.clone(), Some(keypair)).await?;
    }
    Ok(())
}
