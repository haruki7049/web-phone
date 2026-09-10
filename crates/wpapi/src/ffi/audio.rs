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
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let input_devices: Vec<String> = match host.input_devices() {
            Ok(devs) => devs
                .filter_map(|d| d.description().map(|desc| desc.name().to_string()).ok())
                .collect(),
            Err(_) => Vec::new(),
        };
        let output_devices: Vec<String> = match host.output_devices() {
            Ok(devs) => devs
                .filter_map(|d| d.description().map(|desc| desc.name().to_string()).ok())
                .collect(),
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
