# wpapi

`wpapi` is the official core client engine and API reference library (including Foreign Function Interface / C FFI) for the `web-phone` system.

______________________________________________________________________

## Features

- **Core Audio & WebRTC Engine**: Audio capture, playback, resampling, and WebRTC session management.
- **C-Compatible ABI**: Panic-safe (`catch_unwind`), thread-local error handling, and C log callbacks.
- **Header Generation**: Automated C header generation via `cbindgen` (`wpapi.h`).

______________________________________________________________________

## Safety & Memory Rules

1. **Panic Boundary**: All FFI functions are wrapped in `catch_unwind` and return `-1` or `NULL` on error. Panic unwinding never crosses the FFI boundary.
1. **Error Retrieval**: Use `wpapi_last_error_message()` to retrieve thread-local error details upon failure.
1. **Memory Ownership**: Memory allocated by Rust FFI (`WPAPIConfig`, `WPAPICallHandle`, JSON pointers) MUST be freed by corresponding destructor functions (`wpapi_config_free`, `wpapi_call_stop`, `wpapi_string_free`).

______________________________________________________________________

## C Headers & Pkg-Config

C headers and pkg-config manifests are automatically generated during build inside `crates/wpapi/include/`:

- `wpapi.h`: Combined C API header
- `wpapi.pc`: Pkg-config specification

______________________________________________________________________

## C FFI Language Binding Author Guide

When building language bindings for `wpapi` (e.g. Erlang NIFs, Haskell FFI, Python `ctypes`/`cffi`, Node.js N-API, Go `cgo`, C# P/Invoke), adhere to the following rules:

### 1. Asynchronous Callbacks & Threading Model

- `WPAPIEventCallback` is invoked from a Rust Tokio background worker thread when session events occur (`EventAccepted`, `EventRejected`, `EventError`, `EventHangup`, `EventIncomingCall`).
- **Language Runtimes**: Host language runtimes with single-threaded event loops or thread locks (e.g. Python GIL, Node.js V8 main loop, Erlang NIF processes) **MUST NOT** directly run long or runtime-unsafe operations inside the callback. Instead, dispatch/queue the event to the host runtime's event loop (e.g., via `PyGILState_Ensure`, `napi_threadsafe_function`, or `enif_send`).

### 2. Pointer Lifetimes

- The `peer_address` and `detail_message` C string pointers (`const char*`) passed into `WPAPIEventCallback` are valid **ONLY** for the scope/duration of the callback function execution.
- Callers **MUST** copy these strings immediately into native language string objects (e.g., Haskell `Text`, Python `str`, Erlang `binary`) and must **NOT** store the raw pointers.

### 3. Interactive Incoming Call Handling

- When an incoming call request arrives while operating in standby mode, `EventIncomingCall` (4) is emitted via `WPAPIEventCallback`.
- Call `wpapi_call_accept(handle)` to accept the call and begin audio streams, or `wpapi_call_reject(handle)` to reject the incoming request.

### 4. Thread-Safe Error Handling

- Use `wpapi_get_last_error_copy(buf, buf_len)` to copy thread-local error strings into caller-allocated buffers. This prevents race conditions or invalid pointer dereferences in green-thread environments (e.g., Haskell RTS or Go goroutines) where OS threads may migrate.

### 5. In-Call Controls, Non-Blocking Teardown & Free

- Live call handle controls (`wpapi_call_set_muted`, `wpapi_call_is_muted`, `wpapi_call_get_input_level`, `wpapi_call_get_output_level`) are thread-safe and can be invoked concurrently while a call session is active.
- **Non-Blocking Stop**: Call `wpapi_call_request_stop(handle)` to trigger asynchronous teardown without blocking the caller (e.g., main GUI thread).
- **Free Handle**: Call `wpapi_call_free(handle)` (or `wpapi_call_stop(handle)`) to release memory once session cleanup finishes.

______________________________________________________________________

## License

Published under the [MIT License](../../LICENSE).
