#[cfg(target_os = "windows")]
pub fn protect(secret: &[u8]) -> Result<Vec<u8>, String> {
    windows_dpapi(secret, true)
}

#[cfg(target_os = "windows")]
pub fn unprotect(secret: &[u8]) -> Result<Vec<u8>, String> {
    windows_dpapi(secret, false)
}

#[cfg(not(target_os = "windows"))]
pub fn protect(_secret: &[u8]) -> Result<Vec<u8>, String> {
    Err("Secure token storage is currently Windows-only".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn unprotect(_secret: &[u8]) -> Result<Vec<u8>, String> {
    Err("Secure token storage is currently Windows-only".to_string())
}

#[cfg(target_os = "windows")]
fn windows_dpapi(input: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    #[repr(C)]
    struct DataBlob {
        length: u32,
        data: *mut u8,
    }

    #[link(name = "Crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const DataBlob,
            description: *const u16,
            entropy: *const DataBlob,
            reserved: *mut std::ffi::c_void,
            prompt: *const std::ffi::c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const DataBlob,
            description: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *mut std::ffi::c_void,
            prompt: *const std::ffi::c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    let input_blob = DataBlob {
        length: input
            .len()
            .try_into()
            .map_err(|_| "secret is too large for Windows DPAPI".to_string())?,
        data: input.as_ptr() as *mut u8,
    };
    let mut output = DataBlob {
        length: 0,
        data: std::ptr::null_mut(),
    };
    let success = unsafe {
        if protect {
            CryptProtectData(
                &input_blob,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input_blob,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                &mut output,
            )
        }
    };
    if success == 0 || output.data.is_null() {
        return Err("Windows DPAPI could not process the token".to_string());
    }

    let bytes = unsafe {
        let value = std::slice::from_raw_parts(output.data, output.length as usize).to_vec();
        LocalFree(output.data as *mut std::ffi::c_void);
        value
    };
    Ok(bytes)
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::{protect, unprotect};

    #[test]
    fn secret_round_trips_through_windows_dpapi() {
        let encrypted = protect(b"launcher-token").unwrap();
        assert_ne!(encrypted, b"launcher-token");
        assert_eq!(unprotect(&encrypted).unwrap(), b"launcher-token");
    }
}
