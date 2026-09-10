//! Logging and tracing C FFI integration.

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::RwLock;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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
pub type WPAPILogCallback = Option<
    unsafe extern "C" fn(
        level: WPAPILogLevel,
        message: *const c_char,
        user_data: *mut std::ffi::c_void,
    ),
>;

#[derive(Clone, Copy)]
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
        let callback_state = {
            if let Ok(guard) = LOG_CALLBACK.read() {
                *guard
            } else {
                None
            }
        };

        if let Some(state) = callback_state
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

            let user_data = state.user_data;
            let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
                cb(level, c_msg.as_ptr(), user_data);
            }));
        }
    }
}

pub(crate) fn init_tracing_subscriber() {
    let fmt_layer = tracing_subscriber::fmt::layer();
    let _ = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(CallbackLayer)
        .try_init();
}

/// Retrieve the last thread-local error message string.
///
/// # Safety
/// The returned pointer is thread-local and valid until the next FFI error call on the current thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_last_error_message() -> *const c_char {
    unsafe { super::wpapi_get_last_error() }
}

/// Register a custom C log callback function.
/// # Safety
/// `user_data` must remain valid for the duration of registration.
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
#[unsafe(no_mangle)]
pub extern "C" fn wpapi_init() -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        init_tracing_subscriber();
        0
    }))
    .unwrap_or(-1)
}
