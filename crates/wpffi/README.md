# wpffi

`wpffi` is the official core client engine and Foreign Function Interface (FFI) library for the `web-phone` system.

______________________________________________________________________

## Features

- **Core Audio & WebRTC Engine**: Audio capture, playback, resampling, and WebRTC session management.
- **C-Compatible ABI**: Panic-safe (`catch_unwind`), thread-local error handling, and C log callbacks.
- **Header Generation**: Automated C header generation via `cbindgen` (`wpffi.h`, `wpclient.h`, `wpdaemon.h`).

______________________________________________________________________

## Safety & Memory Rules

1. **Panic Boundary**: All FFI functions are wrapped in `catch_unwind` and return `-1` or `NULL` on error. Panic unwinding never crosses the FFI boundary.
1. **Error Retrieval**: Use `wpffi_last_error_message()` to retrieve thread-local error details upon failure.
1. **Memory Ownership**: Memory allocated by Rust FFI (`WPFFIConfig`, `WPFFICallHandle`, JSON pointers) MUST be freed by corresponding destructor functions (`wpffi_config_free`, `wpffi_call_stop`, `wpffi_string_free`).

______________________________________________________________________

## C Headers & Pkg-Config

C headers and pkg-config manifests are automatically generated during build inside `crates/wpffi/include/`:

- `wpffi.h`: Combined C API header
- `wpclient.h`: Client C API header
- `wpdaemon.h`: Daemon C API header
- `wpffi.pc`: Pkg-config specification

______________________________________________________________________

## License

Published under the [MIT License](../../LICENSE).
