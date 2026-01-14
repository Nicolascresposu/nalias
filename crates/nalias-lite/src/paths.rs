use std::fs;
use std::path::{Path, PathBuf};

pub const HOME_OVERRIDE: &str = "NALIAS_LITE_HOME";

#[derive(Debug)]
pub struct AppPaths {
    pub bin: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self, &'static str> {
        let root = if let Some(value) = std::env::var_os(HOME_OVERRIDE) {
            if value.is_empty() {
                return Err("NALIAS_LITE_HOME is set but empty");
            }
            PathBuf::from(value)
        } else {
            let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not available")?;
            PathBuf::from(local).join("NaliasLite")
        };
        Ok(Self {
            bin: root.join("bin"),
        })
    }

    pub fn ensure_directory(&self) -> Result<(), &'static str> {
        reject_reparse(&self.bin)?;
        fs::create_dir_all(&self.bin).map_err(|_| "could not create the alias directory")
    }
}

pub fn reject_reparse(path: &Path) -> Result<(), &'static str> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        let mut reparse = metadata.file_type().is_symlink();
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            reparse |= metadata.file_attributes() & 0x400 != 0;
        }
        if reparse {
            return Err("refusing to use a filesystem reparse point");
        }
    }
    Ok(())
}
