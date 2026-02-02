#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
mod portable {
    use std::path::Path;

    use crate::error::{NaliasError, Result};

    pub fn user_path() -> Result<String> {
        Err(NaliasError::Installation(
            "user PATH integration is only available on Windows".to_owned(),
        ))
    }

    pub fn set_user_path(_: &str) -> Result<()> {
        Err(NaliasError::Installation(
            "user PATH integration is only available on Windows".to_owned(),
        ))
    }

    pub fn broadcast_environment_change() -> Result<()> {
        Ok(())
    }

    pub fn defer_delete(_: &Path, _: &Path) -> Result<()> {
        Err(NaliasError::Installation(
            "deferred deletion is only available on Windows".to_owned(),
        ))
    }
}

#[cfg(not(windows))]
pub use portable::*;
