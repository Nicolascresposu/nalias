use std::path::Path;

#[cfg(windows)]
pub fn open_folder(path: &Path) -> Result<(), &'static str> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: Both strings are valid NUL-terminated UTF-16 buffers. Null optional
    // parameters request the default verb behavior and current working directory.
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

#[cfg(not(windows))]
pub fn open_folder(_: &Path) -> Result<(), &'static str> {
    Err("opening the alias directory is only supported on Windows")
}
