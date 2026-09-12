//! Keystore and address FFI bindings for `wpapi`.

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::session::WPAPIConfig;
use super::set_last_error;
use crate::address::{UserAddress, UserKeypair, build_authorization_header};

/// Query registered user addresses from wpdaemon server as a JSON string array.
/// Caller must free `*out_json` using `wpapi_string_free`.
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
        let endpoint = format!("{}/addresses", cfg.server_url());

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
            let keypair = UserKeypair::generate();
            let (_, auth_hdr) = build_authorization_header(&keypair, "");
            let resp = client
                .get(&endpoint)
                .header(reqwest::header::AUTHORIZATION, auth_hdr)
                .send()
                .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_list_addresses_null_pointers() {
        unsafe {
            assert_eq!(
                wpapi_list_addresses(std::ptr::null(), std::ptr::null_mut()),
                -1
            );
            let mut out = std::ptr::null_mut();
            assert_eq!(wpapi_list_addresses(std::ptr::null(), &mut out), -1);
        }
    }
}
