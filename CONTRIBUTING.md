# Contributing to Web Phone (`web-phone`)

Thank you for your interest in contributing to **`web-phone`**!\
`web-phone` is a high-performance, decentralized WebRTC voice calling system written in Rust, conforming strictly to the **WPIP (Web Phone Improvement Proposals)** specifications (`docs/WPIP-*.md`).

______________________________________________________________________

## 1. Development Environment Setup

### Prerequisites

- **Rust Toolchain**: Rust 2024 edition (managed via `rust-toolchain.toml`).
- **Nix** (Optional but recommended): Nix with Flakes enabled (`nix develop`).
- **System Libraries**: `alsa-lib`, `pkg-config`, `udev` (on Linux systems for CPAL audio playback and capture).

### Setting Up the Environment

Using Nix development shell (recommended):

```bash
nix develop
```

Or using standard Rust tools:

```bash
cargo build
```

______________________________________________________________________

## 2. Project Architecture

The workspace is organized into three core crates and a specifications directory:

- **`crates/wpdaemon`**: WebRTC signaling server daemon, Selective Forwarding Unit (SFU) for group calls (WPIP-08), Keep-Alive heartbeat manager (WPIP-09), rate limiter (WPIP-15), and peer mesh node (WPIP-05).
- **`crates/wpapi`**: Core WebRTC peer connection manager, protocol packet codec (`ProtocolPacket`), CPAL audio capture/playback engine, audio resampler, encrypted keystore (WPIP-14), Nostr/secp256k1 auth (WPIP-16), and C FFI bindings.
- **`crates/wpclient`**: CLI client binary supporting direct 1-to-1 calls (`wpclient call`) and group room calls (`wpclient room`) with TUI interface.
- **`docs/`**: WPIP specifications (`WPIP-01.md` through `WPIP-21.md`).

______________________________________________________________________

## 3. Contribution Guidelines

### Specification Compliance

- All new features and protocol alterations MUST strictly adhere to the RFC 2119 terminology ("MUST", "MUST NOT", "SHOULD", "SHOULD NOT", "REQUIRED", "RECOMMENDED", "MAY") defined in `docs/WPIP-*.md`.
- Maintain backwards compatibility across all supported WPIP DataChannel packet types (`0x01` through `0x13`).

### Concurrency & WebRTC Safety

- `CLIENT_REGISTRY` and shared state are protected by `std::sync::RwLock`. NEVER hold `RwLock` read/write guards across `.await` points when sending over WebRTC DataChannels or performing async I/O.
- Clone necessary `Arc` handles (`RTCDataChannel`, `RTCPeerConnection`, `UserAddress`) inside short synchronous blocks, drop the guard, and then perform `.await` calls.

### Code Formatting (`treefmt`)

Code formatting MUST be performed using `treefmt`:

```bash
treefmt
```

If `treefmt` is not available directly in your execution environment, inspect the `treefmt-nix` configuration in `flake.nix` and apply the corresponding formatters:

- **Rust**: `rustfmt` / `cargo fmt --all`
- **Nix**: `nixfmt`
- **TOML**: `taplo`
- **Shell**: `shfmt`
- **Markdown**: `mdformat`

______________________________________________________________________

## 4. Verification Workflow

Before submitting a Pull Request or opening a commit, all contributions MUST pass the following verification pipeline:

1. **Run Unit & Integration Tests**:
   ```bash
   cargo test --workspace
   ```
1. **Run Linter (Zero Warnings Allowed)**:
   ```bash
   cargo clippy --workspace -- -D warnings
   ```
1. **Build Release Binaries**:
   ```bash
   cargo build --release
   ```
1. **Build Nix Derivation**:
   ```bash
   nix build
   ```
1. **Format Code**:
   ```bash
   treefmt
   ```

______________________________________________________________________

## 5. Submitting Pull Requests

1. Fork or branch from `main`:
   ```bash
   git checkout -b feature/your-feature-name
   ```
1. Make your changes following the coding standards and concurrency guidelines outlined in [`AGENTS.md`](./AGENTS.md).
1. Commit your changes with clear, descriptive commit messages.
1. Push to your branch and open a Pull Request against `main`.
