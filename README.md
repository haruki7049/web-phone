# web-phone

A real-time audio transmission system over WebSocket, written in Rust.

## Overview

web-phone enables real-time voice communication between multiple clients through a WebSocket server. Audio is captured from the microphone, transmitted over WebSocket, and played back on connected clients' speakers.

## Components

- **daemon**: WebSocket server that broadcasts audio data to all connected clients
- **client**: WebSocket client that captures microphone input and plays received audio

## Requirements

### Linux
- ALSA development libraries: `sudo apt-get install libasound2-dev`

## Usage

### Start the Server

```bash
cargo run -p daemon
```

The server listens on `ws://127.0.0.1:15000` by default.

### Start a Client

```bash
# Start an audio call
cargo run -p client -- call

# List available audio devices
cargo run -p client -- list-devices
```

### Configuration

Configuration files are stored in platform-specific directories:
- Linux: `~/.config/web-phone-daemon/config.toml` (server), `~/.config/web-phone-client/config.toml` (client)

#### Server Configuration (`config.toml`)

```toml
ip = "127.0.0.1"
port = 15000
```

#### Client Configuration (`config.toml`)

```toml
server_ip = "127.0.0.1"
server_port = 15000
sample_rate = 48000
channels = 1
```

## Architecture

```
┌─────────┐     Audio Data      ┌─────────┐
│ Client  │ ◄─────────────────► │ Server  │
│  (mic)  │      WebSocket      │(daemon) │
└─────────┘                     └─────────┘
                                    │
                                    │ Broadcast
                                    ▼
                               ┌─────────┐
                               │ Client  │
                               │(speaker)│
                               └─────────┘
```

Multiple clients can connect to the server. Audio from each client is broadcast to all other connected clients.

## License

MIT
