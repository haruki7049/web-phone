# web-phone

A WebSocket-based client-server text chat system written in Rust.

## Architecture

The system consists of two components:

- **Server**: WebSocket server that accepts connections and broadcasts messages to all connected clients
- **Client**: Unified client with message viewer (default) and daemon mode for WebSocket client functionality

## Features

- Real-time message broadcasting to all connected clients
- Persistent message storage in JSON format
- Interactive chat mode
- Simple send command for one-off messages
- View message history

## Usage

### Start the Server

```bash
cargo run -p server
```

By default, the server runs on `ws://127.0.0.1:15000`.

### Send a Message (Daemon Mode)

```bash
cargo run -p client -- --daemon send --message "Your message here"
```

Options:

- `--server`: Server address (default: `127.0.0.1:15000`)
- `--message`: Message to send

### Interactive Chat (Daemon Mode)

```bash
cargo run -p client -- --daemon chat
```

Type messages and press Enter to send. Press Ctrl+C to exit.

Options:

- `--server`: Server address (default: `127.0.0.1:15000`)

### View Message History (Default Mode)

```bash
cargo run -p client
```

Options:

- `--messages-log-path`: Path to messages log file (default: system data directory)

## Message Format

When a client sends a message, the server broadcasts it to all connected clients in the format:

```
[sender_address] message_text
```

All messages are also saved to a JSON log file for persistent storage.
