//! Session control FFI bindings for `wpapi`.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::str::FromStr;
use std::thread::{JoinHandle, spawn};
use tokio::sync::oneshot;

use super::set_last_error;
use crate::address::UserAddress;
use crate::call;
use crate::config::Configuration;

/// Opaque configuration handle for wpclient.
pub struct WPAPIConfig(pub Configuration);

/// Opaque handle representing an active audio call session.
pub struct WPAPICallHandle {
    pub(crate) stop_tx: Option<oneshot::Sender<()>>,
    pub(crate) thread_handle: Option<JoinHandle<()>>,
}

/// Create a new client configuration handle with default settings.
#[unsafe(no_mangle)]
pub extern "C" fn wpapi_config_new() -> *mut WPAPIConfig {
    catch_unwind(AssertUnwindSafe(|| {
        Box::into_raw(Box::new(WPAPIConfig(Configuration::default())))
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// Free a configuration handle created with `wpapi_config_new`.
/// # Safety
/// `config` must be a valid pointer created by `wpapi_config_new`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_config_free(config: *mut WPAPIConfig) {
    if !config.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            let _ = Box::from_raw(config);
        }));
    }
}

/// Set server IP address (IPv4 or IPv6 string) and port.
/// # Safety
/// `config` and `server_ip` must be valid non-null pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_config_set_server(
    config: *mut WPAPIConfig,
    server_ip: *const c_char,
    server_port: u16,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() || server_ip.is_null() {
            set_last_error("Null pointer argument");
            return -1;
        }
        let c_str = unsafe { CStr::from_ptr(server_ip) };
        let ip_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                set_last_error(e);
                return -1;
            }
        };
        let ip_addr = match ip_str.parse() {
            Ok(ip) => ip,
            Err(e) => {
                set_last_error(format!("Invalid IP address '{}': {}", ip_str, e));
                return -1;
            }
        };
        let cfg = unsafe { &mut (*config).0 };
        cfg.server_ip = ip_addr;
        cfg.server_port = server_port;
        0
    }))
    .unwrap_or(-1)
}

/// Set server URL string (e.g. "http://127.0.0.1:15000", "https://daemon.example.com:8443").
/// # Safety
/// `config` and `server_url` must be valid non-null pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_config_set_server_url(
    config: *mut WPAPIConfig,
    server_url: *const c_char,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() || server_url.is_null() {
            set_last_error("Null pointer argument");
            return -1;
        }
        let c_str = unsafe { CStr::from_ptr(server_url) };
        let url_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                set_last_error(e);
                return -1;
            }
        };
        let cfg = unsafe { &mut (*config).0 };
        if let Err(e) = cfg.parse_and_apply_server_url(url_str) {
            set_last_error(e);
            return -1;
        }
        0
    }))
    .unwrap_or(-1)
}

/// Set STUN server URL.
/// # Safety
/// `config` and `stun_server` must be valid non-null pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_config_set_stun_server(
    config: *mut WPAPIConfig,
    stun_server: *const c_char,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() || stun_server.is_null() {
            set_last_error("Null pointer argument");
            return -1;
        }
        let c_str = unsafe { CStr::from_ptr(stun_server) };
        let stun_str = match c_str.to_str() {
            Ok(s) => s.to_string(),
            Err(e) => {
                set_last_error(e);
                return -1;
            }
        };
        let cfg = unsafe { &mut (*config).0 };
        cfg.stun_server = stun_str;
        0
    }))
    .unwrap_or(-1)
}

/// Set auto-accept flag for incoming call requests without CLI prompts.
/// # Safety
/// `config` must be a valid non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_config_set_auto_accept(
    config: *mut WPAPIConfig,
    auto_accept: bool,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() {
            set_last_error("Null pointer argument");
            return -1;
        }
        let cfg = unsafe { &mut (*config).0 };
        cfg.auto_accept = auto_accept;
        0
    }))
    .unwrap_or(-1)
}

/// Set allow-echoback flag.
/// # Safety
/// `config` must be a valid non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_config_set_allow_echoback(
    config: *mut WPAPIConfig,
    allow_echoback: bool,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() {
            set_last_error("Null pointer argument");
            return -1;
        }
        let cfg = unsafe { &mut (*config).0 };
        cfg.allow_echoback = allow_echoback;
        0
    }))
    .unwrap_or(-1)
}

/// Set input/output audio device substring overrides.
/// # Safety
/// `config` must be a valid non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_config_set_audio_devices(
    config: *mut WPAPIConfig,
    input_device: *const c_char,
    output_device: *const c_char,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() {
            set_last_error("Null pointer argument");
            return -1;
        }
        let cfg = unsafe { &mut (*config).0 };
        if !input_device.is_null()
            && let Ok(s) = unsafe { CStr::from_ptr(input_device) }.to_str()
        {
            cfg.input_device = Some(s.to_string());
        }
        if !output_device.is_null()
            && let Ok(s) = unsafe { CStr::from_ptr(output_device) }.to_str()
        {
            cfg.output_device = Some(s.to_string());
        }
        0
    }))
    .unwrap_or(-1)
}

/// Start an audio call session in a background worker thread.
/// # Safety
/// `config` must be a valid non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_call_start(
    config: *const WPAPIConfig,
    target_address: *const c_char,
) -> *mut WPAPICallHandle {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() {
            set_last_error("Null pointer config argument");
            return std::ptr::null_mut();
        }
        let cfg = unsafe { (*config).0.clone() };
        let target_opt = if !target_address.is_null() {
            match unsafe { CStr::from_ptr(target_address) }.to_str() {
                Ok(s) if !s.trim().is_empty() => match UserAddress::from_str(s) {
                    Ok(addr) => Some(addr),
                    Err(e) => {
                        set_last_error(format!("Invalid target address: {}", e));
                        return std::ptr::null_mut();
                    }
                },
                _ => None,
            }
        } else {
            None
        };

        let (stop_tx, stop_rx) = oneshot::channel::<()>();

        let thread_handle = spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("Failed to create tokio runtime for FFI call: {}", e);
                    return;
                }
            };

            rt.block_on(async move {
                if let Err(e) = call::start_call_with_cancel(&cfg, target_opt, Some(stop_rx)).await
                {
                    tracing::error!("Audio call session error: {}", e);
                }
            });
        });

        let handle = Box::new(WPAPICallHandle {
            stop_tx: Some(stop_tx),
            thread_handle: Some(thread_handle),
        });

        Box::into_raw(handle)
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// Start an audio group room call (WPIP-08) in a background worker thread.
/// # Safety
/// `config` must be a valid non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_room_call_start(
    config: *const WPAPIConfig,
    room_address: *const c_char,
) -> *mut WPAPICallHandle {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() || room_address.is_null() {
            set_last_error("Null pointer argument");
            return std::ptr::null_mut();
        }
        let cfg = unsafe { (*config).0.clone() };
        let r_str = match unsafe { CStr::from_ptr(room_address) }.to_str() {
            Ok(s) => s,
            Err(e) => {
                set_last_error(e);
                return std::ptr::null_mut();
            }
        };
        let room_addr = match UserAddress::from_str(r_str) {
            Ok(addr) => addr,
            Err(e) => {
                set_last_error(format!("Invalid room address '{}': {}", r_str, e));
                return std::ptr::null_mut();
            }
        };

        let (stop_tx, stop_rx) = oneshot::channel::<()>();

        let thread_handle = spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("Failed to create tokio runtime for FFI room call: {}", e);
                    return;
                }
            };

            rt.block_on(async move {
                if let Err(e) =
                    call::start_room_call_with_cancel(&cfg, room_addr, Some(stop_rx)).await
                {
                    tracing::error!("Audio room call session error: {}", e);
                }
            });
        });

        let handle = Box::new(WPAPICallHandle {
            stop_tx: Some(stop_tx),
            thread_handle: Some(thread_handle),
        });

        Box::into_raw(handle)
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// Stop and terminate an active call session, freeing its handle.
/// # Safety
/// `handle` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_call_stop(handle: *mut WPAPICallHandle) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            set_last_error("Null handle argument");
            return -1;
        }
        let mut call_handle = unsafe { Box::from_raw(handle) };
        if let Some(stop_tx) = call_handle.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(th) = call_handle.thread_handle.take() {
            let _ = th.join();
        }
        0
    }))
    .unwrap_or(-1)
}
