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

- **`crates/wpdaemon`**: WebRTC signaling server daemon, STUN/TURN UDP server, Selective Forwarding Unit (SFU) for group calls (WPIP-08), Keep-Alive heartbeat manager (WPIP-09), and peer mesh node (WPIP-05).
- **`crates/wpffi`**: Core WebRTC peer connection manager, protocol packet codec (`ProtocolPacket`), CPAL audio capture/playback engine, audio resampler, and C FFI bindings.
- **`crates/wpclient`**: CLI client binary supporting direct 1-to-1 calls (`wpclient call`) and group room calls (`wpclient room`).
- **`docs/`**: WPIP specifications (`WPIP-01.md` through `WPIP-12.md`).

______________________________________________________________________

## 3. Contribution Guidelines

### Specification Compliance

- All new features and protocol alterations MUST strictly adhere to the RFC 2119 terminology ("MUST", "MUST NOT", "SHOULD", "SHOULD NOT", "REQUIRED", "RECOMMENDED", "MAY") defined in `docs/WPIP-*.md`.
- Maintain backwards compatibility across all supported WPIP DataChannel packet types (`0x01` through `0x13`).

### Code Formatting (`treefmt`)

Code formatting MUST be performed using `treefmt`:

```bash
treefmt
```

If `treefmt` is not available in your environment, apply the corresponding formatters configured in `flake.nix`:

- **Rust**: `rustfmt`
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
   cargo clippy --workspace
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
