use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, BufRead, Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use tracing::info;
use tungstenite::{Message, connect};

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

    if args.daemon {
        // Daemon mode: run WebSocket client commands
        match args.action {
            Some(Actions::Send { server, message }) => send_message(&server, &message)?,
            Some(Actions::Chat { server }) => chat(&server)?,
            None => {
                return Err("Daemon mode requires a subcommand (send or chat)".into());
            }
        }
    } else {
        // Default mode: view messages
        let mut log: File = File::open(&args.messages_log_path).unwrap();
        let mut contents: String = String::new();
        log.read_to_string(&mut contents)?;

        if contents.is_empty() {
            // If the log file has no log, Use empty JSON array
            contents.push_str("[]");
        }

        MESSAGES.set(serde_json::from_str(&contents)?).unwrap();
        messages()?;
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

fn send_message(server: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("ws://{}", server);
    let (mut socket, _response) = connect(url)?;

    socket.send(Message::Text(message.to_string().into()))?;
    info!("Message sent: {}", message);

    socket.close(None)?;
    Ok(())
}

fn chat(server: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::thread;

    let url = format!("ws://{}", server);
    let (socket, _response) = connect(url)?;

    info!("Connected to server at {}", server);
    info!("Type messages to send. Press Ctrl+C to exit.");
    println!("\n=== Chat started ===");

    let socket = Arc::new(StdMutex::new(socket));
    let socket_clone = Arc::clone(&socket);

    // Spawn a thread to handle incoming messages
    let _receiver = thread::spawn(move || {
        loop {
            let msg = {
                let mut ws = socket_clone.lock().unwrap();
                ws.read()
            };

            match msg {
                Ok(Message::Text(text)) => {
                    println!("\r{}", text);
                    print!("> ");
                    io::stdout().flush().ok();
                }
                Ok(Message::Close(_)) => {
                    println!("\nServer closed the connection");
                    break;
                }
                Err(_) => break,
                _ => {}
            }
        }
    });

    // Main thread handles user input
    let stdin = io::stdin();
    print!("> ");
    io::stdout().flush()?;

    for line in stdin.lock().lines() {
        if let Ok(message) = line {
            let message = message.trim();
            if !message.is_empty() {
                let mut ws = socket.lock().unwrap();
                ws.send(Message::Text(message.to_string().into()))?;
            }
            print!("> ");
            io::stdout().flush()?;
        }
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
    /// Enable daemon mode for WebSocket client functionality
    #[arg(long)]
    daemon: bool,

    #[clap(subcommand)]
    action: Option<Actions>,

    #[arg(long, default_value = DEFAULT_MESSAGES_LOG_PATH.lock().unwrap().display().to_string())]
    messages_log_path: PathBuf,
}

#[derive(Debug, Clone, Subcommand)]
enum Actions {
    /// Send a single message to the server
    Send {
        #[arg(short, long, default_value = "127.0.0.1:15000")]
        server: String,
        #[arg(short, long)]
        message: String,
    },
    /// Start an interactive chat session with the server
    Chat {
        #[arg(short, long, default_value = "127.0.0.1:15000")]
        server: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogData {
    address: SocketAddr,
    date: DateTime<Utc>,
    text: String,
}
