use clap::Parser;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::thread::spawn;
use tungstenite::{Message, accept};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: CLIArgs = CLIArgs::parse();
    let address: String = format!("{}:{}", args.ip, args.port);

    let server = TcpListener::bind(&address)?;
    eprintln!("Running on ws://{}", &address);
    eprintln!("Use Ctrl-C to stop this program");

    loop {
        let (stream, addr) = server.accept()?;
        read_stream(stream, addr)?;
    }
}

fn read_stream(stream: TcpStream, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    spawn(move || {
        let mut websocket = accept(stream).unwrap();

        loop {
            let message = websocket.read();

            if message.is_err() {
                break;
            }

            match message.unwrap() {
                Message::Text(utf8_bytes) => {
                    let text: &str = utf8_bytes.as_str();
                    println!(
                        "Message from {}: {}",
                        addr,
                        text.strip_suffix("\n").unwrap()
                    );

                    save_message(text, addr).unwrap();
                }
                _ => (),
            }
        }
    });

    Ok(())
}

fn save_message(text: &str, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[derive(Parser)]
struct CLIArgs {
    #[arg(short, long)]
    ip: Ipv4Addr,

    #[arg(short, long)]
    port: u16,
}
