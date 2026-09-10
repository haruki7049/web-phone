//! WebRTC SDP handshake and connection handler.

use crate::config::CONFIGURATION;
use crate::constants::{HANDSHAKE_TIMEOUT, ICE_GATHER_TIMEOUT};
use crate::error::SignalingError;
use crate::registry::CLIENT_REGISTRY;
use async_trait::async_trait;
use axum::{extract::Json, http::HeaderMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceGatheringState, RTCPeerConnectionState, RTCSessionDescription,
};
use wpapi::UserAddress;

use super::datachannel::handle_client_datachannel_events;

/// Counter for connected clients.
static CLIENT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Generate Ephemeral TURN credentials for an authenticated UserAddress (WPIP-10).
pub fn generate_turn_credentials_for_client(
    user_address: &UserAddress,
    ttl_seconds: u64,
) -> wpapi::TurnCredential {
    let daemon_config = CONFIGURATION.get().cloned().unwrap_or_default();
    let secret = crate::config::get_turn_server_secret();
    let turn_urls = vec![format!(
        "turn:{}:{}?transport=udp",
        daemon_config.ip, daemon_config.stun_port
    )];
    wpapi::generate_ephemeral_turn_credential(&secret, user_address, turn_urls, ttl_seconds)
}

struct ClientConnectionHandler {
    client_id: u64,
    user_address: UserAddress,
    my_node_id: u64,
    gather_tx: mpsc::Sender<()>,
}

#[async_trait]
impl PeerConnectionEventHandler for ClientConnectionHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.send(()).await;
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        info!("Client {} PeerConnection state: {}", self.client_id, state);
        if state == RTCPeerConnectionState::Failed
            || state == RTCPeerConnectionState::Closed
            || state == RTCPeerConnectionState::Disconnected
        {
            info!(
                "Client {} ({}) disconnected",
                self.client_id,
                self.user_address.short_id()
            );
            CLIENT_REGISTRY
                .write()
                .unwrap()
                .unregister_client(self.client_id);
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn webrtc::data_channel::DataChannel>) {
        let dc_label = dc.label().await.unwrap_or_default();
        info!(
            "Client {} created DataChannel: {}",
            self.client_id, dc_label
        );

        CLIENT_REGISTRY
            .write()
            .unwrap()
            .data_channels
            .insert(self.client_id, Arc::clone(&dc));

        let client_id = self.client_id;
        let user_address = self.user_address.clone();
        let my_node_id = self.my_node_id;

        tokio::spawn(async move {
            handle_client_datachannel_events(dc, client_id, user_address, my_node_id).await;
        });
    }
}

/// Handle incoming SDP offer from a WebRTC client.
pub async fn handle_sdp_offer(
    headers: HeaderMap,
    Json(offer): Json<RTCSessionDescription>,
) -> Result<Json<RTCSessionDescription>, SignalingError> {
    let daemon_config = CONFIGURATION.get().cloned().unwrap_or_default();
    let my_node_id = daemon_config.node_id;

    let active_connections = CLIENT_REGISTRY.read().unwrap().peer_connections.len();
    if active_connections >= daemon_config.max_connections {
        warn!(
            "Rejected SDP offer: active connections ({}) reached max limit ({})",
            active_connections, daemon_config.max_connections
        );
        return Err(SignalingError::MaxConnectionsReached(
            daemon_config.max_connections,
        ));
    }

    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let user_address = if let Some(auth_val) = auth_header {
        match wpapi::verify_any_authorization_header(auth_val, &offer.sdp) {
            Ok(addr) => {
                info!("Verified client identity signature: {}", addr.short_id());
                addr
            }
            Err(err) => {
                error!("Authorization verification failed: {}", err);
                return Err(SignalingError::Unauthorized(err));
            }
        }
    } else {
        let addr = UserAddress::generate_from_time();
        info!(
            "No Authorization header present. Generated temporary User ID: {}",
            addr
        );
        addr
    };

    let client_id = CLIENT_COUNT.fetch_add(1, Ordering::SeqCst);
    let (gather_tx, mut gather_rx) = mpsc::channel(1);

    let handler = Arc::new(ClientConnectionHandler {
        client_id,
        user_address: user_address.clone(),
        my_node_id,
        gather_tx,
    });

    let config = RTCConfigurationBuilder::default().build();

    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_handler(handler)
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await
        .map_err(|e| {
            SignalingError::InternalError(format!("Failed to build PeerConnection: {}", e))
        })?;

    let peer_connection: Arc<dyn PeerConnection> = Arc::new(pc);

    info!(
        "Client {} connected, assigned User ID: {}",
        client_id, user_address
    );

    CLIENT_REGISTRY.write().unwrap().register_client(
        client_id,
        user_address.clone(),
        Arc::clone(&peer_connection),
    );

    // Handshake timeout task: close connection if DataChannel is not opened within 15s
    let pc_timeout = Arc::clone(&peer_connection);
    tokio::spawn(async move {
        tokio::time::sleep(HANDSHAKE_TIMEOUT).await;
        let has_dc = CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .contains_key(&client_id);
        if !has_dc {
            warn!(
                "Handshake timeout: Client {} failed to open DataChannel within 15s, closing connection",
                client_id
            );
            let _ = pc_timeout.close().await;
            CLIENT_REGISTRY
                .write()
                .unwrap()
                .unregister_client(client_id);
        }
    });

    peer_connection
        .set_remote_description(offer)
        .await
        .map_err(|e| SignalingError::InvalidSdpOffer(e.to_string()))?;

    let answer = peer_connection.create_answer(None).await.map_err(|e| {
        SignalingError::InternalError(format!("Failed to create SDP answer: {}", e))
    })?;

    peer_connection
        .set_local_description(answer)
        .await
        .map_err(|e| {
            SignalingError::InternalError(format!("Failed to set local description: {}", e))
        })?;

    let _ = tokio::time::timeout(ICE_GATHER_TIMEOUT, gather_rx.recv()).await;

    let local_desc = peer_connection.local_description().await.ok_or_else(|| {
        SignalingError::InternalError("No local description available".to_string())
    })?;

    Ok(Json(local_desc))
}
