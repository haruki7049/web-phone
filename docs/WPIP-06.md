# WPIP-06: FFI & Foreign Language Bindings

`draft` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines the **Foreign Function Interface (FFI)** requirements for invoking `web-phone` core capabilities safely from foreign languages (C/C++, Python, Go, Flutter, etc.).

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Safety and Design Principles

The `wpffi` library MUST follow C ABI (`extern "C"`) conventions and adhere to the following safety constraints:

1. **Panic Boundary Protection (`catch_unwind`)**:
   - Rust unwinding panics MUST NOT cross the FFI boundary. All FFI function bodies MUST be wrapped in `catch_unwind` and return `-1` or `NULL` on error.
1. **Thread-Local Error Retrieval (`wpffi_last_error_message`)**:
   - If an FFI function fails, implementation MUST set a human-readable error string in thread-local storage accessible via `wpffi_last_error_message()`.
1. **Memory Ownership**:
   - Memory allocated by Rust FFI (handles `WPFFIConfig`, `WPFFICallHandle`, JSON pointers, etc.) MUST be explicitly freed by corresponding destructor functions (`wpffi_config_free`, `wpffi_call_stop`, `wpffi_string_free`).

______________________________________________________________________

## 2. Core C API Functions

### 2.1 Initialization & Logging

- `wpffi_init() -> c_int`: Initializes tracing subscriber logging.
- `wpffi_set_log_callback(callback: WPFFILogCallback, user_data: *mut c_void)`: Directs log events to a custom C callback.

```c
typedef enum WPFFILogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3
} WPFFILogLevel;

typedef void (*WPFFILogCallback)(enum WPFFILogLevel level, const char *message, void *user_data);
```

### 2.2 Client Configuration & Call APIs

- `wpffi_config_new() -> *mut WPFFIConfig`
- `wpffi_config_free(config: *mut WPFFIConfig)`
- `wpffi_config_set_server(config, ip_str, port)`
- `wpffi_call_start(config, target_address_str) -> *mut WPFFICallHandle`
- `wpffi_call_stop(handle: *mut WPFFICallHandle) -> c_int`

______________________________________________________________________

## 3. C Header Guidelines

The FFI layer SHOULD provide headers for C/C++ developers:

- `include/wpffi.h`: Combined C API header
- `include/wpclient.h`: Client API header (future extension)
- `include/wpdaemon.h`: Daemon API header (future extension)
