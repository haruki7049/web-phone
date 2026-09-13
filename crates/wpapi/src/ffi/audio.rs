//! Audio device FFI bindings for `wpapi`.

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::set_last_error;

/// Free a C string allocated by `wpapi_list_audio_devices` or other FFI calls.
/// # Safety
/// `ptr` must be a pointer allocated by Rust FFI, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            let _ = CString::from_raw(ptr);
        }));
    }
}

/// Query available audio input and output devices as a JSON object string.
/// Caller must free `*out_json` using `wpapi_string_free`.
/// # Safety
/// `out_json` must be a valid non-null pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_list_audio_devices(out_json: *mut *mut c_char) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if out_json.is_null() {
            set_last_error("Null pointer argument");
            return -1;
        }
        let devices = match crate::audio::AudioEngine::list_devices() {
            Ok(devs) => devs,
            Err(e) => {
                set_last_error(e);
                return -1;
            }
        };
        let json_str = serde_json::to_string(&devices).unwrap_or_default();
        match CString::new(json_str) {
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
