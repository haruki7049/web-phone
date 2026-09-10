//! Keep-alive heartbeat task module (WPIP-09).

use crate::registry::CLIENT_REGISTRY;
use bytes::BytesMut;
use tracing::warn;
use wpapi::ProtocolPacket;

/// Start background keep-alive heartbeat task (WPIP-09).
pub fn start_keepalive_task() {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let ping_bytes = BytesMut::from(
                ProtocolPacket::Ping { timestamp: now_ms }
                    .encode()
                    .as_slice(),
            );

            let channels = {
                let reg = CLIENT_REGISTRY.read().unwrap();
                reg.data_channels.clone()
            };

            for (_cid, dc) in channels {
                let _ = dc.send(ping_bytes.clone()).await;
            }

            let stale_cids = {
                let reg = CLIENT_REGISTRY.read().unwrap();
                reg.get_stale_clients(30)
            };

            for cid in stale_cids {
                warn!(
                    "Client {} failed keep-alive pong response for >30s, terminating connection",
                    cid
                );
                let pc = {
                    let reg = CLIENT_REGISTRY.read().unwrap();
                    reg.peer_connections.get(&cid).cloned()
                };
                if let Some(pc) = pc {
                    let _ = pc.close().await;
                }
                CLIENT_REGISTRY.write().unwrap().unregister_client(cid);
            }
        }
    });
}
