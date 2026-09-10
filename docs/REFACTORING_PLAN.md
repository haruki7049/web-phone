# Web-Phone Architecture Refactoring Plan

This document tracks the execution of codebase architecture refactorings for Issues #43 through #48.

- [ ] #43: Split `wpdaemon/src/connection.rs` into dedicated submodules
- [ ] #44: Decouple UI rendering, event handling, and state in `wpclient/src/tui.rs`
- [ ] #45: Separate C FFI bindings into `wpapi/src/ffi.rs`
- [ ] #46: Define offset constants and separate decoder functions in `wpapi/src/protocol.rs`
- [ ] #47: Centralize Daemon HTTP client and authorization header handling
- [ ] #48: Introduce domain-specific error enums using `thiserror`
