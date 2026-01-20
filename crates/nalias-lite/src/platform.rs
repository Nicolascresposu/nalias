use std::path::Path;

#[cfg(windows)]
mod windows {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, RegCloseKey,
        RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: This guard owns the registry handle.
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }

    fn environment_key() -> Result<Key, &'static str> {
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
            Err("could not open the user environment registry key")
        }
    }

    pub fn user_path() -> Result<String, &'static str> {
        let key = environment_key()?;
        let name = wide("Path");
        let mut kind = 0;
        let mut size = 0;
        // SAFETY: This query requests only the required buffer size.
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
            return Err("could not read the user PATH");
        }
        let mut bytes = vec![0u8; size as usize];
        // SAFETY: bytes has the size returned by the first query.
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
            return Err("could not read the user PATH");
        }
        let units: Vec<u16> = bytes[..size as usize]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();
        String::from_utf16(&units).map_err(|_| "the user PATH is invalid UTF-16")
    }

    pub fn set_user_path(value: &str) -> Result<(), &'static str> {
        let key = environment_key()?;
        let name = wide("Path");
        let data = wide(value);
        let byte_length =
            u32::try_from(data.len() * 2).map_err(|_| "the user PATH is too large")?;
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
            Err("could not update the user PATH")
        }
    }

    pub fn broadcast_environment_change() -> Result<(), &'static str> {
        let environment = wide("Environment");
        let mut ignored = 0;
        // SAFETY: The UTF-16 buffer remains alive for this synchronous call.
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
            Err("the environment-change notification failed")
        } else {
            Ok(())
        }
    }

    pub fn open_folder(path: &Path) -> Result<(), &'static str> {
        let operation = wide("open");
        let path = wide(path.as_os_str());
        // SAFETY: Both strings are valid NUL-terminated UTF-16 buffers.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result as isize <= 32 {
            Err("could not open the alias directory")
        } else {
            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod portable {
    use std::path::Path;

    pub fn user_path() -> Result<String, &'static str> {
        Err("PATH installation is only supported on Windows")
    }

    pub fn set_user_path(_: &str) -> Result<(), &'static str> {
        Err("PATH installation is only supported on Windows")
    }

    pub fn broadcast_environment_change() -> Result<(), &'static str> {
        Ok(())
    }

    pub fn open_folder(_: &Path) -> Result<(), &'static str> {
        Err("opening the alias directory is only supported on Windows")
    }
}

#[cfg(not(windows))]
use portable as implementation;
#[cfg(windows)]
use windows as implementation;

pub fn user_path() -> Result<String, &'static str> {
    implementation::user_path()
}

pub fn set_user_path(value: &str) -> Result<(), &'static str> {
    implementation::set_user_path(value)
}

pub fn broadcast_environment_change() -> Result<(), &'static str> {
    implementation::broadcast_environment_change()
}

pub fn open_folder(path: &Path) -> Result<(), &'static str> {
    implementation::open_folder(path)
}
