//! ABI v1 for native hosts. The existing BridgeClient owns IPC and helper lifetime.
use crate::client::BridgeClient;
use crate::protocol::{Request, Response};
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::time::Duration;

fn response(f: impl FnOnce() -> Result<Response, String>) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        let result = f().unwrap_or_else(|message| Response::error(-1, message));
        CString::new(serde_json::to_string(&result).expect("Response serialization"))
            .expect("JSON has no literal NUL")
            .into_raw()
    }))
    .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn j2534_abi_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn j2534_client_create(timeout_ms: u32) -> *mut BridgeClient {
    if !(1..=60_000).contains(&timeout_ms) {
        return ptr::null_mut();
    }
    catch_unwind(|| {
        let mut client = BridgeClient::new();
        client.set_response_timeout(Duration::from_millis(timeout_ms.into()));
        Box::into_raw(Box::new(client))
    })
    .unwrap_or(ptr::null_mut())
}

/// # Safety
/// `client` must be null or a live, exclusively owned handle from create.
#[no_mangle]
pub unsafe extern "C" fn j2534_client_destroy(client: *mut BridgeClient) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !client.is_null() {
            drop(Box::from_raw(client));
        }
    }));
}

/// # Safety
/// `client` must be null or a live handle, not in use by another thread.
#[no_mangle]
pub unsafe extern "C" fn j2534_client_start(client: *mut BridgeClient, bitness: u8) -> *mut c_char {
    response(|| {
        let client = client.as_mut().ok_or("Null bridge client")?;
        if bitness != 32 && bitness != 64 {
            return Err("Bitness must be 32 or 64".into());
        }
        if client.is_running() {
            return Err("Bridge already running".into());
        }
        if let Err(error) = client.start(bitness) {
            // start can fail after spawning; never leave that child behind.
            let _ = client.stop();
            return Err(error);
        }
        Ok(Response::ok_none())
    })
}

/// # Safety
/// `client` must be null or an exclusive live handle. `request_json` must be
/// null or point to a readable NUL-terminated string for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn j2534_client_request(
    client: *mut BridgeClient,
    request_json: *const c_char,
) -> *mut c_char {
    response(|| {
        let client = client.as_mut().ok_or("Null bridge client")?;
        if request_json.is_null() {
            return Err("Null request".into());
        }
        let text = CStr::from_ptr(request_json)
            .to_str()
            .map_err(|e| e.to_string())?;
        let request: Request = serde_json::from_str(text).map_err(|e| e.to_string())?;
        client.send_request(request)
    })
}

/// # Safety
/// `text` must be null or an unfreed string returned by this API.
#[no_mangle]
pub unsafe extern "C" fn j2534_string_free(text: *mut c_char) {
    if !text.is_null() {
        drop(CString::from_raw(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn take(text: *mut c_char) -> serde_json::Value {
        assert!(!text.is_null());
        let result = serde_json::from_str(CStr::from_ptr(text).to_str().unwrap()).unwrap();
        j2534_string_free(text);
        result
    }
    #[test]
    fn abi_ownership_and_invalid_arguments() {
        unsafe {
            assert_eq!(j2534_abi_version(), 1);
            assert!(j2534_client_create(0).is_null());
            assert!(j2534_client_create(60_001).is_null());
            j2534_client_destroy(ptr::null_mut());
            j2534_string_free(ptr::null_mut());
            assert_eq!(
                take(j2534_client_start(ptr::null_mut(), 64))["status"],
                "error"
            );
            let client = j2534_client_create(100);
            assert!(!client.is_null());
            assert_eq!(take(j2534_client_start(client, 16))["status"], "error");
            for bytes in [
                b"{\0".as_slice(),
                b"{\"method\":\"EnumerateDevices\"}\0",
                b"\xff\0",
            ] {
                assert_eq!(
                    take(j2534_client_request(client, bytes.as_ptr().cast()))["status"],
                    "error"
                );
            }
            assert_eq!(
                take(j2534_client_request(client, ptr::null()))["status"],
                "error"
            );
            j2534_client_destroy(client);
        }
    }
}
