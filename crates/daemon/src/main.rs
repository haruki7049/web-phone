use clap::{Parser, Subcommand};
use std::io::{self, BufRead, Write};
use tracing::info;
use tungstenite::{Message, connect};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args: CLIArgs = CLIArgs::parse();

    match args.action {
        Actions::Send { server, message } => send_message(&server, &message)?,
        Actions::Chat { server } => chat(&server)?,
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

#[derive(Debug, Parser)]
#[clap(version, author, about = env!("CARGO_PKG_DESCRIPTION"))]
struct CLIArgs {
    #[clap(subcommand)]
    action: Actions,
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
