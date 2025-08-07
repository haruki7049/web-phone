use directories::ProjectDirs;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Write, Read};
use std::fs::File;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::thread::spawn;
use tracing::{debug, info};
use tungstenite::{Message, accept};

const LOG_FILENAME: &str = "log.json";

pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");
    let mut config_path: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    config_path.push(filename);
    Mutex::new(config_path)
});
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

#[tracing::instrument]
pub fn read_stream(stream: TcpStream, address: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    spawn(move || {
        let mut websocket = accept(stream).unwrap();

        loop {
            let message = websocket.read();

            if message.is_err() {
                break;
            }

            match message.unwrap() {
                Message::Text(utf8_bytes) => {
                    let text: String = utf8_bytes.as_str().to_string();
                    info!(
                        "Message from {}: {}",
                        address,
                        text.strip_suffix("\n").unwrap()
                    );

                    save_message(text, address).unwrap();
                }
                Message::Close(v) => match v {
                    Some(close_frame) => {
                        info!("{} is closed by: {}", address, close_frame);
                        break;
                    }
                    None => {
                        info!("{} is closed without any reason", address);
                        break;
                    }
                },
                _ => (),
            }
        }
    });

    Ok(())
}

#[tracing::instrument]
fn save_message(text: String, address: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or("Failed to get Configuration from CONFIGURATION")?;

    debug!("config: {:?}", config);

    std::fs::create_dir_all(&config.log_dir)?;
    let mut filepath: PathBuf = config.log_dir.clone();
    filepath.push(LOG_FILENAME);

    let now = Utc::now();
    let log_data: LogData = LogData {
        address: address,
        date: now,
        text: text.clone(),
    };

    match std::fs::exists(&filepath) {
        Ok(true) => {
            let mut original_log: File = File::open(&filepath)?;
            let mut original_contents: String = String::new();
            original_log.read_to_string(&mut original_contents)?;

            let mut log_data_list: Vec<LogData> = serde_json::from_str(&original_contents)?;
            log_data_list.push(log_data);

            let log_data_string: String = serde_json::to_string(&log_data_list)?;
            let mut new_log: File = File::create(&filepath)?;
            let bytes: &[u8] = log_data_string.as_bytes();
            new_log.write_all(bytes)?;
        }
        Ok(false) => {
            let log_data_list: Vec<LogData> = vec![log_data];

            let mut log: File = File::create(filepath)?;
            let log_data_string: String = serde_json::to_string(&log_data_list)?;
            let bytes: &[u8] = log_data_string.as_bytes();
            log.write_all(bytes)?;
        }
        Err(_) => return Err(Box::new(SaveMessageError::new("Failed to get original log file".to_string()))),
    }

    Ok(())
}

#[derive(Debug)]
struct SaveMessageError {
    err_message: String,
}

impl std::fmt::Display for SaveMessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error from log saving process: {}", self.err_message)
    }
}

impl std::error::Error for SaveMessageError {}

impl SaveMessageError {
    pub fn new(err_message: String) -> Self {
        Self { err_message }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub log_dir: PathBuf,
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl std::default::Default for Configuration {
    fn default() -> Self {
        Self {
            log_dir: default_log_dir(),
            ip: Ipv4Addr::new(127, 0, 0, 1),
            port: 15000,
        }
    }
}

fn default_log_dir() -> PathBuf {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");

    proj_dirs.data_dir().to_path_buf()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogData {
    address: SocketAddr,
    date: DateTime<Utc>,
    text: String,
}
