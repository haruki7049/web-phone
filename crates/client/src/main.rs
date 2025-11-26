use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, StreamConfig};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use tracing::{error, info};
use tungstenite::{Message, connect};

static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-client")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-client");
    let mut result: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    result.push(filename);
    Mutex::new(result)
});

static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

/// Audio buffer for receiving audio data from WebSocket (FIFO queue)
static AUDIO_BUFFER: LazyLock<Mutex<VecDeque<f32>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args: CLIArgs = CLIArgs::parse();

    CONFIGURATION
        .set(confy::load_path(&args.config_path).unwrap_or_else(|_| {
            info!("Running web-phone-client with default Configuration...");
            Configuration::default()
        }))
        .unwrap();

    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or("Failed to get Configuration")?;

    match args.action {
        Actions::Call => start_audio_call(config)?,
        Actions::ListDevices => list_audio_devices()?,
    }

    Ok(())
}

fn list_audio_devices() -> Result<(), Box<dyn std::error::Error>> {
    let host = cpal::default_host();

    info!("Available input devices:");
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            info!("  - {}", name);
        }
    }

    info!("Available output devices:");
    for device in host.output_devices()? {
        if let Ok(name) = device.name() {
            info!("  - {}", name);
        }
    }

    Ok(())
}

fn start_audio_call(config: &Configuration) -> Result<(), Box<dyn std::error::Error>> {
    let server_url = format!("ws://{}:{}", config.server_ip, config.server_port);
    info!("Connecting to audio server at {}...", server_url);

    let (websocket, _) = connect(&server_url)?;
    let websocket = Arc::new(Mutex::new(websocket));

    info!("Connected to audio server");
    info!("Press Ctrl-C to disconnect");

    // Set up audio host
    let host = cpal::default_host();

    // Set up input (microphone)
    let input_device = host
        .default_input_device()
        .ok_or("No input device available")?;
    info!("Using input device: {}", input_device.name()?);

    // Set up output (speakers)
    let output_device = host
        .default_output_device()
        .ok_or("No output device available")?;
    info!("Using output device: {}", output_device.name()?);

    // Configure audio format - use a common format
    let sample_rate = SampleRate(config.sample_rate);
    let channels = config.channels;

    let stream_config = StreamConfig {
        channels,
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    // Clone websocket for input stream
    let ws_for_input = Arc::clone(&websocket);

    // Create input stream (capture from microphone)
    let input_stream = input_device.build_input_stream(
        &stream_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // Convert f32 samples to bytes for transmission
            let bytes: Vec<u8> = data
                .iter()
                .flat_map(|&sample| sample.to_le_bytes())
                .collect();

            // Send audio data over WebSocket
            if let Ok(mut ws) = ws_for_input.lock()
                && let Err(e) = ws.send(Message::Binary(bytes.into()))
            {
                error!("Failed to send audio: {}", e);
            }
        },
        |err| error!("Input stream error: {}", err),
        None,
    )?;

    // Create output stream (play to speakers)
    let output_stream = output_device.build_output_stream(
        &stream_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut buffer = AUDIO_BUFFER.lock().unwrap();

            // Fill output buffer with received audio (FIFO) or silence
            for sample in data.iter_mut() {
                *sample = buffer.pop_front().unwrap_or(0.0);
            }
        },
        |err| error!("Output stream error: {}", err),
        None,
    )?;

    // Start streams
    input_stream.play()?;
    output_stream.play()?;

    info!("Audio streams started. Speaking now will transmit audio.");

    // Handle incoming audio in main thread
    loop {
        let message = {
            let mut ws = websocket.lock().unwrap();
            ws.read()
        };

        match message {
            Ok(Message::Binary(audio_data)) => {
                // Convert received bytes back to f32 samples
                let samples: Vec<f32> = audio_data
                    .chunks_exact(4)
                    .map(|chunk| {
                        let bytes: [u8; 4] = chunk.try_into().unwrap();
                        f32::from_le_bytes(bytes)
                    })
                    .collect();

                // Add to playback buffer (FIFO order for proper audio playback)
                let mut buffer = AUDIO_BUFFER.lock().unwrap();
                for sample in samples {
                    buffer.push_back(sample);
                }
            }
            Ok(Message::Close(_)) => {
                info!("Connection closed by server");
                break;
            }
            Ok(Message::Ping(data)) => {
                let mut ws = websocket.lock().unwrap();
                let _ = ws.send(Message::Pong(data));
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

#[derive(Debug, Parser)]
#[clap(version, author, about = env!("CARGO_PKG_DESCRIPTION"))]
struct CLIArgs {
    #[clap(subcommand)]
    action: Actions,

    #[arg(long, default_value = DEFAULT_CONFIG_PATH.lock().unwrap().display().to_string())]
    config_path: PathBuf,
}

#[derive(Debug, Clone, clap::Subcommand)]
enum Actions {
    /// Start an audio call with the server
    Call,
    /// List available audio devices
    ListDevices,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Configuration {
    server_ip: Ipv4Addr,
    server_port: u16,
    sample_rate: u32,
    channels: u16,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            server_ip: Ipv4Addr::new(127, 0, 0, 1),
            server_port: 15000,
            sample_rate: 48000,
            channels: 1,
        }
    }
}
