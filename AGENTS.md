# AGENTS.md - Web Phone Project Guidelines

This file provides instructions and guidelines for AI agents working in this codebase.

## 1. Project Overview

`web-phone` is a high-performance, decentralized WebRTC voice calling system written in Rust, conforming to the **WPIP (Web Phone Improvement Proposals)** specifications (`docs/WPIP-*.md`).

### Workspace Architecture

- **`crates/wpdaemon`**: WebRTC signaling server daemon, STUN/TURN UDP server, Selective Forwarding Unit (SFU) for group calls (WPIP-08), Keep-Alive heartbeat manager (WPIP-09), and peer mesh node (WPIP-05).
- **`crates/wpffi`**: Core WebRTC peer connection manager, protocol packet codec (`ProtocolPacket`), CPAL audio capture/playback engine, audio resampler, and C FFI bindings.
- **`crates/wpclient`**: CLI client binary supporting direct 1-to-1 calls (`wpclient call`) and group room calls (`wpclient room`).
- **`docs/`**: WPIP specifications (`WPIP-01.md` through `WPIP-12.md`).

______________________________________________________________________

## 2. Specification Compliance

- All code MUST strictly comply with RFC 2119 terminology ("MUST", "MUST NOT", "SHOULD", "SHOULD NOT", "REQUIRED", "RECOMMENDED", "MAY") as defined in `docs/WPIP-*.md`.
- Maintain backwards compatibility across all supported WPIP packet types (`0x01` through `0x13`).

______________________________________________________________________

## 3. Coding Guidelines & Practices

- **Rust Standards**: Use modern Rust idioms (`as_chunks`, `is_multiple_of`, `let-else`, etc.).
- **Concurrency & WebRTC Safety**:
  - `CLIENT_REGISTRY` is protected by `std::sync::RwLock`. NEVER hold `RwLock` read/write guards across `.await` points when sending over WebRTC DataChannels or performing async I/O.
  - Clone necessary `Arc` handles (`RTCDataChannel`, `RTCPeerConnection`, `UserAddress`) inside short synchronous blocks, drop the guard, and then perform async `.await` calls.
- **Error Handling**: Preserve full log tracebacks and return proper `Result` types. Do not mask errors with superficial fallbacks or silent swallows.
- **Documentation**: Retain existing doc comments and docstrings.

______________________________________________________________________

## 4. Verification Workflow

Before declaring any task or feature complete, agents MUST run and pass:

1. `cargo test --workspace` - All unit and integration tests must pass cleanly.
1. `cargo clippy --workspace` - Zero warnings allowed.
1. `cargo build --release` - Release binaries must build cleanly.
1. `nix build` - Nix flake derivation build must succeed.
1. `treefmt` - Code formatting MUST be performed using `treefmt` (installed via `nix develop`). If `treefmt` is not directly available in the execution environment, inspect the `treefmt-nix` configuration defined in `flake.nix` and manually apply the corresponding formatters for each file type (e.g., `rustfmt` for Rust, `nixfmt` for Nix, `taplo` for TOML, `shfmt` for Shell scripts, `mdformat` for Markdown).
