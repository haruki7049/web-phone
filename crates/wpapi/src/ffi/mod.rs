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
use std::os::raw::c_char;

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
