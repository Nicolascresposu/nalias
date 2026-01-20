use std::fs;
use std::path::{Path, PathBuf};

pub const HOME_OVERRIDE: &str = "NALIAS_LITE_HOME";

#[derive(Debug)]
pub struct AppPaths {
    pub root: PathBuf,
    pub bin: PathBuf,
    pub executable: PathBuf,
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
            executable: root.join("nalias-lite.exe"),
            root,
        })
    }

    pub fn is_overridden() -> bool {
        std::env::var_os(HOME_OVERRIDE).is_some()
    }

    pub fn ensure_directories(&self) -> Result<(), &'static str> {
        reject_reparse(&self.root)?;
        fs::create_dir_all(&self.root)
            .map_err(|_| "could not create the installation directory")?;
        reject_reparse(&self.bin)?;
        fs::create_dir_all(&self.bin).map_err(|_| "could not create the alias directory")
    }

    pub fn same_path(&self, left: &Path, right: &Path) -> bool {
        match (left.canonicalize(), right.canonicalize()) {
            (Ok(left), Ok(right)) => left == right,
            _ => left
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy()),
        }
    }

    pub fn files_equal(&self, left: &Path, right: &Path) -> Result<bool, &'static str> {
        let left_metadata = fs::metadata(left).map_err(|_| "could not inspect this executable")?;
        let right_metadata =
            fs::metadata(right).map_err(|_| "could not inspect the installed executable")?;
        if left_metadata.len() != right_metadata.len() {
            return Ok(false);
        }
        let left = fs::read(left).map_err(|_| "could not read this executable")?;
        let right = fs::read(right).map_err(|_| "could not read the installed executable")?;
        Ok(left == right)
    }
}

fn normalized_path(path: &Path) -> String {
    let value = path
        .as_os_str()
        .to_string_lossy()
        .trim()
        .trim_matches('"')
        .replace('/', "\\");
    value.trim_end_matches('\\').to_ascii_lowercase()
}

fn path_contains(value: &str, entry: &Path) -> bool {
    let expected = normalized_path(entry);
    value
        .split(';')
        .any(|item| normalized_path(Path::new(item)) == expected)
}

pub fn add_path_entry(value: &str, entry: &Path) -> String {
    if path_contains(value, entry) {
        return value.to_owned();
    }
    if value.is_empty() {
        entry.to_string_lossy().into_owned()
    } else if value.ends_with(';') {
        format!("{value}{}", entry.display())
    } else {
        format!("{value};{}", entry.display())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_addition_is_case_insensitive_and_idempotent() {
        let value = r"C:\Windows;C:\NaliasLite\bin";
        assert_eq!(
            add_path_entry(value, Path::new(r"c:\naliaslite\bin\")),
            value
        );
    }
}
