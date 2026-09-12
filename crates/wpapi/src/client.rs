//! Daemon HTTP API Client module for `wpapi`.
//!
//! Provides a centralized, reusable client for interacting with the `wpdaemon` HTTP endpoints,
//! managing identity keypair creation, signature generation for authorization headers, and response parsing.

use crate::address::{UserAddress, UserKeypair, build_authorization_header};
use crate::config::Configuration;
use anyhow::{Context, Result};

/// Reusable HTTP API client for querying `wpdaemon` endpoints.
#[derive(Debug, Clone)]
pub struct DaemonApiClient {
    client: reqwest::Client,
}

impl Default for DaemonApiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonApiClient {
    /// Create a new `DaemonApiClient`.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Fetch registered user addresses from daemon (`/addresses` endpoint).
    pub async fn fetch_registered_addresses(
        &self,
        config: &Configuration,
    ) -> Result<Vec<UserAddress>> {
        let endpoint = format!("{}/addresses", config.server_url());
        let keypair = UserKeypair::generate();
        let (_, auth_hdr) = build_authorization_header(&keypair, "");

        let resp = self
            .client
            .get(&endpoint)
            .header(reqwest::header::AUTHORIZATION, auth_hdr)
            .send()
            .await
            .context("Failed to send HTTP request to daemon /addresses endpoint")?;

        if !resp.status().is_success() {
            anyhow::bail!("Daemon returned HTTP status {}", resp.status());
        }

        let addresses = resp
            .json::<Vec<UserAddress>>()
            .await
            .context("Failed to parse addresses JSON response from daemon")?;

        Ok(addresses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_api_client_instantiation() {
        let client = DaemonApiClient::new();
        let _default_client = DaemonApiClient::default();
        let _cloned = client.clone();
    }

    #[tokio::test]
    async fn test_fetch_registered_addresses_unreachable_server() {
        let client = DaemonApiClient::new();
        let mut config = Configuration::default();
        config.server.address = "127.0.0.1:1".into(); // Unreachable port

        let res = client.fetch_registered_addresses(&config).await;
        assert!(res.is_err());
    }
}
