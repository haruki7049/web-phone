# wpclient

`wpclient` is the official reference client implementation for the `web-phone` real-time WebRTC audio system.

It implements the client-side specifications defined in WPIPs:

- **[WPIP-03](../../docs/WPIP-03.md)**: `wpclient` Core Specification
- **[WPIP-04](../../docs/WPIP-04.md)**: DataChannel Wire Protocol
- **[WPIP-08](../../docs/WPIP-08.md)**: Call Control & Capacity Constraints

______________________________________________________________________

## Features

- **Standby & Call Modes**: Wait for incoming calls or initiate immediate outbound calls.
- **Audio Device Management**: Automatic input/output audio device selection and high-quality 48kHz resampling via `cpal`.
- **Interactive & Auto-Accept**: Interactive call acceptance CLI prompts with `auto_accept` configuration support.
- **Cross-Platform**: Configured via `~/.config/wpclient/config.toml`.

______________________________________________________________________

## Usage

### 1. Launch in Standby Mode (Wait for Calls)

```bash
cargo run -p wpclient
```

### 2. Initiate an Outbound Call

```bash
cargo run -p wpclient -- call --to <TARGET_USER_ADDRESS>
```

### 3. List Audio Input & Output Devices

```bash
cargo run -p wpclient -- list-devices
```

______________________________________________________________________

## License

Published under the [MIT License](../../LICENSE).
