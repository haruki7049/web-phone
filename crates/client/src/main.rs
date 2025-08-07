use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use tracing::info;

static DEFAULT_MESSAGES_LOG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone");
    let mut result: PathBuf = proj_dirs.data_dir().to_path_buf();
    let filename: &str = "messages.json";

    result.push(filename);
    Mutex::new(result)
});

static MESSAGES: OnceLock<Vec<LogData>> = OnceLock::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args: CLIArgs = CLIArgs::parse();

    let mut log: File = File::open(&args.messages_log_path)?;
    let mut contents: String = String::new();
    log.read_to_string(&mut contents)?;
    MESSAGES.set(serde_json::from_str(&contents)?).unwrap();

    match args.action {
        Actions::Messages => messages()?,
    }

    Ok(())
}

fn messages() -> Result<(), Box<dyn std::error::Error>> {
    let messages: &Vec<LogData> = MESSAGES.get().ok_or("Failed to get Messages")?;
    if messages.is_empty() {
        return Err(Box::new(Errors::NoMessages));
    }

    for message in messages {
        info!("message: {:?}", message);
    }

    Ok(())
}

#[derive(Debug)]
enum Errors {
    NoMessages,
}

impl std::error::Error for Errors {}

impl std::fmt::Display for Errors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Errors::NoMessages => write!(f, "No messages"),
        }
    }
}

#[derive(Debug, Parser)]
#[clap(version, author, about = env!("CARGO_PKG_DESCRIPTION"))]
struct CLIArgs {
    #[clap(subcommand)]
    action: Actions,

    #[arg(long, default_value = DEFAULT_MESSAGES_LOG_PATH.lock().unwrap().display().to_string())]
    messages_log_path: PathBuf,
}

#[derive(Debug, Clone, Subcommand)]
enum Actions {
    Messages,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogData {
    address: SocketAddr,
    date: DateTime<Utc>,
    text: String,
}
