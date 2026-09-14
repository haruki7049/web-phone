# AGENTS.md - Web Phone Project Guidelines

This file provides instructions and guidelines for AI agents working in this codebase.

## 1. Project Overview

`web-phone` is a high-performance, decentralized WebRTC voice calling system written in Rust, conforming to the **WPIP (Web Phone Improvement Proposals)** specifications (`docs/WPIP-*.md`).

### Workspace Architecture

- **`crates/wpdaemon`**: WebRTC signaling server daemon, Selective Forwarding Unit (SFU) for group calls (WPIP-08), Keep-Alive heartbeat manager (WPIP-09), rate limiter (WPIP-15), and peer mesh node (WPIP-05).
- **`crates/wpapi`**: Core WebRTC peer connection manager, protocol packet codec (`ProtocolPacket`), CPAL audio capture/playback engine, audio resampler, encrypted keystore (WPIP-14), Nostr/secp256k1 auth (WPIP-16), video frame transport (WPIP-20), SFrame E2EE (WPIP-11), and C FFI bindings.
- **`crates/wpclient`**: CLI client binary supporting direct 1-to-1 calls (`wpclient call`) and group room calls (`wpclient room`) with TUI interface.
- **`docs/`**: WPIP specifications (`WPIP-01.md` through `WPIP-21.md`).

______________________________________________________________________

## 2. Specification Compliance

- All code MUST strictly comply with RFC 2119 terminology ("MUST", "MUST NOT", "SHOULD", "SHOULD NOT", "REQUIRED", "RECOMMENDED", "MAY") as defined in `docs/WPIP-*.md`.
- Maintain backwards compatibility across all supported WPIP packet types (`0x01` through `0x14`).

______________________________________________________________________

## 3. Coding Guidelines & Practices

- **Rust Standards**: Use modern Rust idioms (`as_chunks`, `is_multiple_of`, `let-else`, etc.).
- **Concurrency & WebRTC Safety**:
  - `CLIENT_REGISTRY` and shared state are protected by `std::sync::RwLock`. NEVER hold `RwLock` read/write guards across `.await` points when sending over WebRTC DataChannels or performing async I/O.
  - Clone necessary `Arc` handles (`RTCDataChannel`, `RTCPeerConnection`, `UserAddress`) inside short synchronous blocks, drop the guard, and then perform async `.await` calls.
- **Error Handling**: Preserve full log tracebacks and return proper `Result` types. Do not mask errors with superficial fallbacks or silent swallows.
- **Issue Verification**: Before starting any task or feature implementation, agents MUST check relevant GitHub Issues and existing discussions to confirm requirements and prevent duplicate or redundant work.
- **No Unsolicited Execution on Possibility Inquiries**: When the user asks whether an action or task is possible (e.g., "Is it possible to...?"), agents MUST NOT execute the action automatically (such as creating/modifying GitHub issues, modifying labels, or executing destructive/modifying commands). Agents MUST ONLY answer whether it is possible, explain the method, and present proposed options, and MUST WAIT for explicit user confirmation before executing.
- **Commit Message Standards**: All commit messages MUST strictly adhere to the [Conventional Commits](https://www.conventionalcommits.org/) specification (e.g., `feat:`, `fix:`, `refactor:`, `perf:`, `docs:`, `test:`, `build:`, `ci:`, `chore:`).
- **Issue Estimate Guidelines**:
  - Issue Estimates MUST be derived from expected AI agent token usage, using a **30pt upper limit**.
  - **Estimate Scale**:
    - `30pt`: Maximum complexity (500k – 1M+ tokens). Cross-crate architectures or complex protocol state machines. (Tasks >1M tokens MUST be decomposed).
    - `20pt`: High complexity (300k – 500k tokens). Extensive single-crate logic, complex E2EE session management.
    - `13pt`: Medium-high complexity (150k – 300k tokens). Auto-reconnection / ICE restart, resampler additions, lock refactorings.
    - `8pt`: Medium complexity (80k – 150k tokens). Audio device selection UI, dynamic config hot-reloading.
    - `5pt`: Low-medium complexity (40k – 80k tokens). Text input modal controls, packet drop metrics tracking.
    - `3pt`: Low complexity (20k – 40k tokens). Persistent file logging, cache-line padding optimizations.
    - `1pt`: Very low complexity (<20k tokens). C FFI lifetime docs, minor comment/typo fixes.
  - Estimates MUST be tracked using the GitHub Projects custom `Estimate` numeric field.
- **Documentation**: Retain existing doc comments and docstrings.

______________________________________________________________________

## 4. Verification Workflow

Before declaring any task or feature complete, agents MUST run and pass:

1. `cargo test --workspace` - All unit and integration tests must pass cleanly.
1. `cargo clippy --workspace` - Zero warnings allowed.
1. `cargo build --release` - Release binaries must build cleanly.
1. `nix build` - Nix flake derivation build must succeed.
1. `treefmt` - Code formatting MUST be performed using `treefmt` (installed via `nix develop`). If `treefmt` is not directly available in the execution environment, inspect the `treefmt-nix` configuration defined in `flake.nix` and manually apply the corresponding formatters for each file type (e.g., `rustfmt` for Rust, `nixfmt` for Nix, `taplo` for TOML, `shfmt` for Shell scripts, `mdformat` for Markdown).
1. `cargo llvm-cov --workspace` - Test coverage MUST be checked using `cargo-llvm-cov`.

______________________________________________________________________

## 5. Proactive Maintenance & Specification Verification

When no new feature requests or implementation tasks are specified by the user:

- **Refactoring Existing Code**: Proactively identify, propose, and implement refactoring opportunities (e.g., reducing boilerplate, eliminating duplicate logic, enhancing DRY principles, adopting modern Rust idioms like `let-else`, and improving module encapsulation).
- **WPIP Specification Compatibility Verification**: Audit and verify full compatibility with WPIP specifications (`docs/WPIP-*.md`). Ensure packet formats (`0x01` through `0x14`), authorization headers, rate-limiting rules, energy routing, and encryption algorithms strictly conform to the spec definitions across `wpapi`, `wpdaemon`, and `wpclient`.
