//! C FFI bindings and log callback integrations for `wpapi`.

pub mod audio;
pub mod keystore;
pub mod logging;
pub mod session;

pub use audio::*;
pub use keystore::*;
pub use logging::*;
pub use session::*;

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_char, c_int};

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

pub(crate) fn set_last_error(err: impl std::fmt::Display) {
    let cstr =
        CString::new(err.to_string()).unwrap_or_else(|_| CString::new("Unknown error").unwrap());
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = Some(cstr);
    });
}

/// Retrieve the last thread-local C FFI error message.
///
/// # Safety
/// The returned pointer is managed thread-locally and remains valid until the next FFI call on the same thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_get_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null())
    })
}

/// Copy the last thread-local C FFI error message into a caller-supplied buffer.
/// Returns 0 on success, or -1 if no error is present or buffer is invalid.
///
/// # Safety
/// `buf` must be a valid pointer to a writeable memory buffer of at least `buf_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wpapi_get_last_error_copy(buf: *mut c_char, buf_len: usize) -> c_int {
    if buf.is_null() || buf_len == 0 {
        return -1;
    }
    LAST_ERROR.with(|cell| {
        if let Some(ref cstr) = *cell.borrow() {
            let bytes = cstr.as_bytes_with_nul();
            let copy_len = std::cmp::min(bytes.len(), buf_len);
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, copy_len);
                *buf.add(copy_len - 1) = 0;
            }
            0
        } else {
            unsafe { *buf = 0 };
            -1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_last_error() {
        unsafe {
            // Initially null
            assert!(wpapi_get_last_error().is_null());
            assert_eq!(wpapi_get_last_error_copy(std::ptr::null_mut(), 0), -1);

            set_last_error("Test FFI error message");
            let ptr = wpapi_get_last_error();
            assert!(!ptr.is_null());
            let cstr = std::ffi::CStr::from_ptr(ptr);
            assert_eq!(cstr.to_str().unwrap(), "Test FFI error message");

            let mut buffer = [0i8; 128];
            assert_eq!(
                wpapi_get_last_error_copy(buffer.as_mut_ptr(), buffer.len()),
                0
            );
            let cstr_copied = std::ffi::CStr::from_ptr(buffer.as_ptr());
            assert_eq!(cstr_copied.to_str().unwrap(), "Test FFI error message");
        }
    }
}
