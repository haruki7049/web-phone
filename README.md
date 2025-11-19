# web-phone

A WebSocket-based client-server text chat system written in Rust.

## Architecture

The system consists of two components:
- **Daemon (Server)**: WebSocket server that accepts connections and broadcasts messages to all connected clients
- **Client**: Command-line client for sending messages, interactive chat, and viewing message history

## Features

- Real-time message broadcasting to all connected clients
- Persistent message storage in JSON format
- Interactive chat mode
- Simple send command for one-off messages
- View message history

## Usage

### Start the Server

```bash
cargo run -p daemon
```

By default, the server runs on `ws://127.0.0.1:15000`.

### Send a Message

```bash
cargo run -p client -- send --message "Your message here"
```

Options:
- `--server`: Server address (default: `127.0.0.1:15000`)
- `--message`: Message to send

### Interactive Chat

```bash
cargo run -p client -- chat
```

Type messages and press Enter to send. Press Ctrl+C to exit.

Options:
- `--server`: Server address (default: `127.0.0.1:15000`)

### View Message History

```bash
cargo run -p client -- messages
```

Options:
- `--messages-log-path`: Path to messages log file (default: system data directory)

## Message Format

When a client sends a message, the server broadcasts it to all connected clients in the format:
```
[sender_address] message_text
```

All messages are also saved to a JSON log file for persistent storage.
