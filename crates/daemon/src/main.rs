use clap::Parser;
use std::net::TcpListener;
use std::net::Ipv4Addr;
use std::thread::spawn;
use tungstenite::accept;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: CLIArgs = CLIArgs::parse();
    let address: String = format!("{}:{}", args.ip, args.port);

    eprintln!("Running on ws://{}", &address);

    let server = TcpListener::bind(&address)?;
    for stream in server.incoming() {
        spawn(move || {
            let mut websocket = accept(stream.unwrap()).unwrap();

            loop {
                let msg = websocket.read().unwrap();

                if msg.is_binary() || msg.is_text() {
                    websocket.send(msg).unwrap();
                }
            }
        });
    }

    Ok(())
}

#[derive(Parser)]
struct CLIArgs {
    #[arg(short, long)]
    ip: Ipv4Addr,

    #[arg(short, long)]
    port: u16,
}
