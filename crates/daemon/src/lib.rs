use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use tracing::info;
use tungstenite::{Message, WebSocket, accept};

pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");
    let mut config_path: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    config_path.push(filename);
    Mutex::new(config_path)
});
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

/// Type alias for a WebSocket client connection
type Client = Arc<Mutex<WebSocket<TcpStream>>>;

/// Global list of all connected WebSocket clients for broadcasting audio
pub static CONNECTED_CLIENTS: LazyLock<Mutex<Vec<Client>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Handle incoming WebSocket stream and broadcast audio data to all other clients
#[tracing::instrument(skip(stream))]
pub fn read_stream(
    stream: TcpStream,
    address: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    std::thread::spawn(move || {
        let websocket = match accept(stream) {
            Ok(ws) => ws,
            Err(e) => {
                tracing::error!("Failed to accept WebSocket connection from {}: {}", address, e);
                return;
            }
        };

        let client = Arc::new(Mutex::new(websocket));

        // Register client
        {
            let mut clients = CONNECTED_CLIENTS.lock().unwrap();
            clients.push(Arc::clone(&client));
            info!("Client {} connected. Total clients: {}", address, clients.len());
        }

        loop {
            let message = {
                let mut ws = client.lock().unwrap();
                ws.read()
            };

            match message {
                Ok(Message::Binary(audio_data)) => {
                    // Broadcast audio data to all other connected clients
                    broadcast_audio(&audio_data, &client);
                }
                Ok(Message::Close(frame)) => {
                    match frame {
                        Some(close_frame) => {
                            info!("{} disconnected: {}", address, close_frame);
                        }
                        None => {
                            info!("{} disconnected without reason", address);
                        }
                    }
                    break;
                }
                Ok(Message::Ping(data)) => {
                    let mut ws = client.lock().unwrap();
                    let _ = ws.send(Message::Pong(data));
                }
                Err(e) => {
                    tracing::error!("Error reading from {}: {}", address, e);
                    break;
                }
                _ => {}
            }
        }

        // Unregister client
        {
            let mut clients = CONNECTED_CLIENTS.lock().unwrap();
            clients.retain(|c| !Arc::ptr_eq(c, &client));
            info!("Client {} removed. Total clients: {}", address, clients.len());
        }
    });

    Ok(())
}

/// Broadcast audio data to all connected clients except the sender
fn broadcast_audio(
    audio_data: &[u8],
    sender: &Client,
) {
    let clients = CONNECTED_CLIENTS.lock().unwrap();
    let message = Message::Binary(audio_data.to_vec().into());

    for client in clients.iter() {
        // Don't send back to the sender
        if Arc::ptr_eq(client, sender) {
            continue;
        }

        if let Ok(mut ws) = client.lock()
            && let Err(e) = ws.send(message.clone())
        {
            tracing::warn!("Failed to send audio to client: {}", e);
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl std::default::Default for Configuration {
    fn default() -> Self {
        Self {
            ip: Ipv4Addr::new(127, 0, 0, 1),
            port: 15000,
        }
    }
}
