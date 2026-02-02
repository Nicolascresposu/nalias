use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_DELAY_UNTIL_REBOOT, MoveFileExW};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, RegCloseKey,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};

use crate::error::{NaliasError, Result};

const ENVIRONMENT_KEY: &str = "Environment";
const PATH_VALUE: &str = "Path";

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        // SAFETY: The handle was returned by RegOpenKeyExW and is owned by this guard.
        unsafe { RegCloseKey(self.0) };
    }
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

fn open_environment_key() -> Result<RegistryKey> {
    let subkey = wide(ENVIRONMENT_KEY);
    let mut key = std::ptr::null_mut();
    // SAFETY: `subkey` is NUL-terminated and `key` is a valid out pointer.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(NaliasError::Installation(format!(
            "could not open HKEY_CURRENT_USER\\Environment: {}",
            std::io::Error::from_raw_os_error(status as i32)
        )));
    }
    Ok(RegistryKey(key))
}

pub fn user_path() -> Result<String> {
    let key = open_environment_key()?;
    let name = wide(PATH_VALUE);
    let mut data_type = 0;
    let mut size = 0u32;
    // SAFETY: All pointers are valid for the documented query-only call.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            std::ptr::null(),
            &mut data_type,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(String::new());
    }
    if status != ERROR_SUCCESS {
        return Err(NaliasError::Installation(format!(
            "could not read the user PATH: {}",
            std::io::Error::from_raw_os_error(status as i32)
        )));
    }
    if size == 0 {
        return Ok(String::new());
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: `data` has `size` bytes and all other pointers remain valid.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            std::ptr::null(),
            &mut data_type,
            data.as_mut_ptr(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(NaliasError::Installation(format!(
            "could not read the user PATH: {}",
            std::io::Error::from_raw_os_error(status as i32)
        )));
    }
    data.truncate(size as usize);
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16(&units)
        .map_err(|e| NaliasError::Installation(format!("the user PATH is not valid UTF-16: {e}")))
}

pub fn set_user_path(value: &str) -> Result<()> {
    let key = open_environment_key()?;
    let name = wide(PATH_VALUE);
    let data: Vec<u16> = OsStr::new(value).encode_wide().chain(Some(0)).collect();
    let bytes = data.len().checked_mul(2).ok_or_else(|| {
        NaliasError::Installation("the resulting user PATH is too large".to_owned())
    })?;
    let bytes = u32::try_from(bytes).map_err(|_| {
        NaliasError::Installation("the resulting user PATH is too large".to_owned())
    })?;
    // SAFETY: `name` and `data` are NUL-terminated and the byte length is correct.
    let status = unsafe {
        RegSetValueExW(
            key.0,
            name.as_ptr(),
            0,
            REG_EXPAND_SZ,
            data.as_ptr().cast(),
            bytes,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(NaliasError::Installation(format!(
            "could not update the user PATH: {}",
            std::io::Error::from_raw_os_error(status as i32)
        )));
    }
    Ok(())
}

pub fn broadcast_environment_change() -> Result<()> {
    let environment = wide("Environment");
    let mut ignored = 0usize;
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
        return Err(NaliasError::Installation(format!(
            "PATH was updated, but the environment-change notification failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

pub fn defer_delete(path: &Path, cleanup_root: &Path) -> Result<()> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = std::env::temp_dir().join(format!(
        "nalias-cleanup-{}-{}.cmd",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));
    let escaped_path = path.to_string_lossy().replace('%', "%%");
    let escaped_parent = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_string_lossy()
        .replace('%', "%%");
    let escaped_root = cleanup_root.to_string_lossy().replace('%', "%%");
    let contents = format!(
        "@echo off\r\nfor /L %%N in (1,1,10) do (\r\n  del /f /q \"{escaped_path}\" >nul 2>&1\r\n  if not exist \"{escaped_path}\" goto done\r\n  ping 127.0.0.1 -n 2 >nul\r\n)\r\n:done\r\nrmdir \"{escaped_parent}\" >nul 2>&1\r\nrmdir \"{escaped_root}\" >nul 2>&1\r\ndel /f /q \"%~f0\" >nul 2>&1\r\n"
    );
    fs::write(&script, contents)
        .map_err(|e| NaliasError::io("could not create deferred cleanup script", e))?;
    match Command::new("cmd.exe")
        .args(["/D", "/V:OFF", "/C", "call"])
        .arg(&script)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(_) => Ok(()),
        Err(spawn_error) => {
            let _ = fs::remove_file(&script);
            let wide_path = wide(path.as_os_str());
            // SAFETY: `wide_path` is NUL-terminated; a null destination requests deletion.
            let result = unsafe {
                MoveFileExW(
                    wide_path.as_ptr(),
                    std::ptr::null(),
                    MOVEFILE_DELAY_UNTIL_REBOOT,
                )
            };
            if result == 0 {
                Err(NaliasError::Installation(format!(
                    "could not start deferred cleanup ({spawn_error}) or schedule reboot cleanup ({})",
                    std::io::Error::last_os_error()
                )))
            } else {
                Ok(())
            }
        }
    }
}
