#[cfg(windows)]
mod implementation {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, RegCloseKey,
        RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: This guard owns the handle returned by RegOpenKeyExW.
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }

    fn environment_key() -> Result<Key, String> {
        let name = wide("Environment");
        let mut key = std::ptr::null_mut();
        // SAFETY: The name is NUL-terminated and key is a valid output pointer.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                name.as_ptr(),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                &mut key,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(Key(key))
        } else {
            Err(format!(
                "could not open the user environment registry key: {}",
                std::io::Error::from_raw_os_error(status as i32)
            ))
        }
    }

    pub fn user_path() -> Result<String, String> {
        let key = environment_key()?;
        let name = wide("Path");
        let mut kind = 0;
        let mut size = 0;
        // SAFETY: This first query requests only the required buffer size.
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(String::new());
        }
        if status != ERROR_SUCCESS {
            return Err(format!(
                "could not read the user PATH: {}",
                std::io::Error::from_raw_os_error(status as i32)
            ));
        }
        let mut bytes = vec![0u8; size as usize];
        // SAFETY: bytes has the size reported by the preceding registry query.
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                bytes.as_mut_ptr(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(format!(
                "could not read the user PATH: {}",
                std::io::Error::from_raw_os_error(status as i32)
            ));
        }
        let units: Vec<u16> = bytes[..size as usize]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();
        String::from_utf16(&units).map_err(|error| format!("user PATH is invalid UTF-16: {error}"))
    }

    pub fn set_user_path(value: &str) -> Result<(), String> {
        let key = environment_key()?;
        let name = wide("Path");
        let data = wide(value);
        let byte_length = u32::try_from(data.len() * 2)
            .map_err(|_| "the resulting user PATH is too large".to_owned())?;
        // SAFETY: name and data are valid NUL-terminated UTF-16 buffers.
        let status = unsafe {
            RegSetValueExW(
                key.0,
                name.as_ptr(),
                0,
                REG_EXPAND_SZ,
                data.as_ptr().cast(),
                byte_length,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "could not update the user PATH: {}",
                std::io::Error::from_raw_os_error(status as i32)
            ))
        }
    }

    pub fn broadcast_environment_change() -> Result<(), String> {
        let environment = wide("Environment");
        let mut ignored = 0;
        // SAFETY: The buffer remains alive for this synchronous call.
        let result = unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5_000,
                &mut ignored,
            )
        };
        if result == 0 {
            Err(format!(
                "environment-change notification failed: {}",
                std::io::Error::last_os_error()
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod implementation {
    pub fn user_path() -> Result<String, String> {
        Err("PATH installation is only supported on Windows".to_owned())
    }

    pub fn set_user_path(_: &str) -> Result<(), String> {
        Err("PATH installation is only supported on Windows".to_owned())
    }

    pub fn broadcast_environment_change() -> Result<(), String> {
        Ok(())
    }
}

pub use implementation::*;
