//! C API bindings and core client library for web-phone WebRTC audio system.

pub mod address;
pub mod audio;
pub mod call;
pub mod config;
pub mod keystore;
pub mod protocol;
pub mod resample;
pub mod session;
pub mod turn_auth;
pub mod webrtc_session;

pub use address::{
    UserAddress, UserKeypair, verify_authorization_header, verify_secp256k1_authorization_header,
};
pub use audio::AudioEngine;
pub use config::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
pub use keystore::{
    EncryptedKeyStore, get_default_keystore_path, load_encrypted_keystore, save_encrypted_keystore,
};
pub use protocol::{ProtocolError, ProtocolPacket};
pub use session::ClientSession;
pub use turn_auth::{
    TurnCredential, generate_ephemeral_turn_credential, verify_ephemeral_turn_credential,
};

use cpal::traits::{DeviceTrait, HostTrait};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::str::FromStr;
use std::thread::{JoinHandle, spawn};
use tokio::sync::oneshot;

use std::sync::RwLock;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Log levels for WPAPI log callback.
/// 0 = DEBUG, 1 = INFO, 2 = WARN, 3 = ERROR
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WPAPILogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

/// Function pointer type for log callbacks.
/// # Parameters
/// - `level`: Log level enum (`WPAPILogLevel`).
/// - `message`: Null-terminated C string containing the log message.
/// - `user_data`: User-provided opaque pointer passed when registering the callback.
pub type WPAPILogCallback = Option<
    unsafe extern "C" fn(
        level: WPAPILogLevel,
        message: *const c_char,
        user_data: *mut std::ffi::c_void,
    ),
>;

struct LogCallbackState {
    callback: WPAPILogCallback,
    user_data: *mut std::ffi::c_void,
}

unsafe impl Send for LogCallbackState {}
unsafe impl Sync for LogCallbackState {}

static LOG_CALLBACK: RwLock<Option<LogCallbackState>> = RwLock::new(None);

#[derive(Default)]
struct StringVisitor {
    message: String,
}

impl tracing::field::Visit for StringVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else {
            if !self.message.is_empty() {
                self.message.push_str(", ");
            }
            self.message
                .push_str(&format!("{}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            if !self.message.is_empty() {
                self.message.push_str(", ");
            }
            self.message
                .push_str(&format!("{}={}", field.name(), value));
        }
    }
}

struct CallbackLayer;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CallbackLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if let Ok(guard) = LOG_CALLBACK.read()
            && let Some(state) = guard.as_ref()
            && let Some(cb) = state.callback
        {
            let level = match *event.metadata().level() {
                tracing::Level::TRACE | tracing::Level::DEBUG => WPAPILogLevel::Debug,
                tracing::Level::INFO => WPAPILogLevel::Info,
                tracing::Level::WARN => WPAPILogLevel::Warn,
                tracing::Level::ERROR => WPAPILogLevel::Error,
            };

            let mut visitor = StringVisitor::default();
            event.record(&mut visitor);

            let c_msg = match CString::new(visitor.message) {
                Ok(s) => s,
                Err(_) => CString::new("Log message contained null bytes").unwrap(),
            };

            unsafe {
                cb(level, c_msg.as_ptr(), state.user_data);
            }
        }
    }
}

fn init_tracing_subscriber() {
    let fmt_layer = tracing_subscriber::fmt::layer();
    let _ = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(CallbackLayer)
        .try_init();
}

fn set_last_error(err: impl std::fmt::Display) {
    let err_str = err.to_string();
    let c_str = CString::new(err_str)
        .unwrap_or_else(|_| CString::new("Error containing null bytes").unwrap());
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = Some(c_str);
    });
}

/// Retrieve the last thread-local error message string if any C API call returned non-zero error status.
/// The returned pointer is managed internally and must NOT be freed by the caller.
#[unsafe(no_mangle)]
pub extern "C" fn wpapi_last_error_message() -> *const c_char {
    LAST_ERROR.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null())
    })
}

/// Register a custom C log callback function to receive log messages.
/// Pass `None` (or `NULL` in C) as `callback` to disable log callbacks.
/// # Safety
/// `user_data` must be valid for the duration of callbacks, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_set_log_callback(
    callback: WPAPILogCallback,
    user_data: *mut std::ffi::c_void,
) {
    let mut guard = LOG_CALLBACK.write().unwrap();
    if callback.is_some() {
        *guard = Some(LogCallbackState {
            callback,
            user_data,
        });
    } else {
        *guard = None;
    }
    init_tracing_subscriber();
}

/// Initialize tracing subscriber for logging output.
/// Returns 0 on success, or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn wpapi_init() -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        init_tracing_subscriber();
        0
    }))
    .unwrap_or(-1)
}

/// Opaque configuration handle for wpclient.
pub struct WPAPIConfig(pub Configuration);

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
/// Returns 0 on success, or -1 on error.
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

/// Set STUN server URL (e.g., "stun:127.0.0.1:3478").
/// Returns 0 on success, or -1 on error.
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
/// Returns 0 on success, or -1 on error.
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

/// Set allow-echoback flag (hear own voice).
/// Returns 0 on success, or -1 on error.
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

/// Set input/output audio device substring overrides (pass NULL to leave unchanged or use default).
/// Returns 0 on success, or -1 on error.
/// # Safety
/// `config` must be a valid non-null pointer. `input_device` and `output_device` must be valid C strings or NULL.
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

/// Query registered user addresses from wpdaemon server as a JSON string array.
/// Caller must free `*out_json` using `wpapi_string_free`.
/// Returns 0 on success, or -1 on error.
/// # Safety
/// `config` and `out_json` must be valid non-null pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_list_addresses(
    config: *const WPAPIConfig,
    out_json: *mut *mut c_char,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() || out_json.is_null() {
            set_last_error("Null pointer argument");
            return -1;
        }
        let cfg = unsafe { &(*config).0 };
        let server_url = match cfg.server_ip {
            std::net::IpAddr::V4(ip) => format!("http://{}:{}", ip, cfg.server_port),
            std::net::IpAddr::V6(ip) => format!("http://[{}]:{}", ip, cfg.server_port),
        };
        let endpoint = format!("{}/addresses", server_url);

        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                set_last_error(e);
                return -1;
            }
        };

        let res: Result<String, anyhow::Error> = rt.block_on(async {
            let client = reqwest::Client::new();
            let resp = client.get(&endpoint).send().await?;
            if !resp.status().is_success() {
                anyhow::bail!("Server returned HTTP status {}", resp.status());
            }
            let addresses: Vec<UserAddress> = resp.json().await?;
            let json_str = serde_json::to_string(&addresses)?;
            Ok(json_str)
        });

        match res {
            Ok(json_str) => match CString::new(json_str) {
                Ok(c_str) => {
                    unsafe { *out_json = c_str.into_raw() };
                    0
                }
                Err(e) => {
                    set_last_error(e);
                    -1
                }
            },
            Err(e) => {
                set_last_error(e);
                -1
            }
        }
    }))
    .unwrap_or(-1)
}

/// Query available audio input and output devices as a JSON object.
/// Caller must free `*out_json` using `wpapi_string_free`.
/// Returns 0 on success, or -1 on error.
/// # Safety
/// `out_json` must be a valid non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_list_audio_devices(out_json: *mut *mut c_char) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if out_json.is_null() {
            set_last_error("Null pointer argument");
            return -1;
        }
        let host = cpal::default_host();
        let input_devices: Vec<String> = match host.input_devices() {
            Ok(devs) => devs.filter_map(|d| d.name().ok()).collect(),
            Err(_) => Vec::new(),
        };
        let output_devices: Vec<String> = match host.output_devices() {
            Ok(devs) => devs.filter_map(|d| d.name().ok()).collect(),
            Err(_) => Vec::new(),
        };
        let val = serde_json::json!({
            "input_devices": input_devices,
            "output_devices": output_devices,
        });
        match CString::new(val.to_string()) {
            Ok(c_str) => {
                unsafe { *out_json = c_str.into_raw() };
                0
            }
            Err(e) => {
                set_last_error(e);
                -1
            }
        }
    }))
    .unwrap_or(-1)
}

/// Opaque handle representing an active audio call session.
pub struct WPAPICallHandle {
    stop_tx: Option<oneshot::Sender<()>>,
    thread_handle: Option<JoinHandle<()>>,
}

/// Start an audio call session in a background worker thread.
/// `target_address` can be NULL to operate in standby mode, or a valid UserAddress SHA-256 string.
/// Returns pointer to `WPAPICallHandle` on success, or NULL on error.
/// # Safety
/// `config` must be a valid non-null pointer. `target_address` must be a valid C string or NULL.
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
/// `room_address` must be a valid UserAddress SHA-256 string for the target room.
/// Returns pointer to `WPAPICallHandle` on success, or NULL on error.
/// # Safety
/// `config` must be a valid non-null pointer. `room_address` must be a valid C string pointer.
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
/// Returns 0 on success, or -1 on error.
/// # Safety
/// `handle` must be a valid pointer returned by `wpapi_call_start`.
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

/// Free a C string allocated by `wpapi_list_addresses` or `wpapi_list_audio_devices`.
/// # Safety
/// `ptr` must be a pointer allocated by `wpapi_list_addresses` or `wpapi_list_audio_devices`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            let _ = CString::from_raw(ptr);
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wpapi_config_lifecycle() {
        let config = wpapi_config_new();
        assert!(!config.is_null());

        let server = CString::new("127.0.0.1").unwrap();
        let res = unsafe { wpapi_config_set_server(config, server.as_ptr(), 15000) };
        assert_eq!(res, 0);

        let res = unsafe { wpapi_config_set_auto_accept(config, true) };
        assert_eq!(res, 0);

        let res = unsafe { wpapi_config_set_allow_echoback(config, false) };
        assert_eq!(res, 0);

        unsafe { wpapi_config_free(config) };
    }

    #[test]
    fn test_wpapi_list_audio_devices() {
        let mut json_ptr: *mut c_char = std::ptr::null_mut();
        let res = unsafe { wpapi_list_audio_devices(&mut json_ptr) };
        assert_eq!(res, 0);
        assert!(!json_ptr.is_null());

        let json_cstr = unsafe { CStr::from_ptr(json_ptr) };
        let json_str = json_cstr.to_str().unwrap();
        assert!(json_str.contains("input_devices"));
        assert!(json_str.contains("output_devices"));

        unsafe { wpapi_string_free(json_ptr) };
    }

    #[test]
    fn test_wpapi_log_callback() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static LOG_CALLED: AtomicBool = AtomicBool::new(false);

        unsafe extern "C" fn custom_log_cb(
            level: WPAPILogLevel,
            msg: *const c_char,
            _user_data: *mut std::ffi::c_void,
        ) {
            assert!(
                level == WPAPILogLevel::Info
                    || level == WPAPILogLevel::Error
                    || level == WPAPILogLevel::Debug
                    || level == WPAPILogLevel::Warn
            );
            assert!(!msg.is_null());
            LOG_CALLED.store(true, Ordering::Relaxed);
        }

        unsafe { wpapi_set_log_callback(Some(custom_log_cb), std::ptr::null_mut()) };

        tracing::info!("Test log message for callback");

        assert!(LOG_CALLED.load(Ordering::Relaxed));

        unsafe { wpapi_set_log_callback(None, std::ptr::null_mut()) };
    }
}
