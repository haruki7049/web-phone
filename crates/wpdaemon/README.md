# wpdaemon

`wpdaemon` is the official reference server daemon implementation for the `web-phone` real-time WebRTC audio system.

It implements the daemon-side specifications defined in WPIPs:

- **[WPIP-02](../../docs/WPIP-02.md)**: `wpdaemon` Core Specification
- **[WPIP-04](../../docs/WPIP-04.md)**: DataChannel Wire Protocol
- **[WPIP-05](../../docs/WPIP-05.md)**: Inter-Daemon Peer Mesh Federation
- **[WPIP-07](../../docs/WPIP-07.md)**: Call Control & Capacity Constraints

______________________________________________________________________

## Features

- **WebRTC Signaling Server**: HTTP REST endpoints (`/sdp`, `/peer/sdp`, `/addresses`) for WebRTC handshake.
- **Peer Mesh Interconnection**: Multi-node daemon mesh federation with loop prevention.
- **Call Capacity Enforcement**: Strict 2-participant capacity enforcement and call request routing.
- **Configuration**: Configured via `~/.config/wpdaemon/config.toml`.

______________________________________________________________________

## Usage

### 1. Run Server Daemon

```bash
cargo run -p wpdaemon
```

### 2. Override Port Settings

```bash
cargo run -p wpdaemon -- --port 15000
```

### 3. Connect to Peer Mesh Nodes

```bash
cargo run -p wpdaemon -- --peer http://192.168.1.50:15000
```

______________________________________________________________________

## License

Published under the [MIT License](../../LICENSE).
