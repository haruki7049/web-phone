# wpffi

`wpffi` is the official core client engine and Foreign Function Interface (FFI) library for the `web-phone` system.

It implements:
- **[WPIP-06](../../docs/WPIP-06.md)**: FFI & Foreign Language Bindings

---

## Features

- **Core Audio & WebRTC Engine**: Audio capture, playback, resampling, and WebRTC session management.
- **C-Compatible ABI**: Panic-safe (`catch_unwind`), thread-local error handling, and C log callbacks.
- **Header Generation**: Automated C header generation via `cbindgen` (`wpffi.h`, `wpclient.h`, `wpdaemon.h`).

---

## C Headers & Pkg-Config

C headers and pkg-config manifests are automatically generated during build inside `crates/wpffi/include/`:
- `wpffi.h`: Combined C API header
- `wpclient.h`: Client C API header
- `wpdaemon.h`: Daemon C API header
- `wpffi.pc`: Pkg-config specification

---

## License

Published under the [MIT License](../../LICENSE).
