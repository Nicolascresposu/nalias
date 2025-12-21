use std::path::{Path, PathBuf};

use crate::error::{NaliasError, Result};

pub const HOME_OVERRIDE: &str = "NALIAS_HOME";

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub bin: PathBuf,
    pub executable: PathBuf,
    pub lock: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self> {
        let root = if let Some(value) = std::env::var_os(HOME_OVERRIDE) {
            if value.is_empty() {
                return Err(NaliasError::Installation(format!(
                    "{HOME_OVERRIDE} is set but empty"
                )));
            }
            PathBuf::from(value)
        } else {
            let local = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
                NaliasError::Installation("LOCALAPPDATA is not available".to_owned())
            })?;
            PathBuf::from(local).join("Nalias")
        };
        Ok(Self::from_root(root))
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self {
            config: root.join("aliases.json"),
            bin: root.join("bin"),
            executable: root.join("nalias.exe"),
            lock: root.join("aliases.lock"),
            root,
        }
    }

    pub fn is_overridden() -> bool {
        std::env::var_os(HOME_OVERRIDE).is_some()
    }
}

pub fn normalized_path(path: &Path) -> String {
    let text = path
        .as_os_str()
        .to_string_lossy()
        .trim()
        .trim_matches('"')
        .replace('/', "\\");
    let trimmed = text.trim_end_matches('\\');
    if trimmed.len() == 2 && trimmed.as_bytes()[1] == b':' {
        format!("{trimmed}\\").to_ascii_lowercase()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

pub fn path_contains(path_value: &str, needle: &Path) -> bool {
    let wanted = normalized_path(needle);
    path_value
        .split(';')
        .any(|entry| normalized_path(Path::new(entry)) == wanted)
}

pub fn add_path_entry(path_value: &str, entry: &Path) -> String {
    if path_contains(path_value, entry) {
        return path_value.to_owned();
    }
    let entry = entry.to_string_lossy();
    if path_value.is_empty() {
        entry.into_owned()
    } else if path_value.ends_with(';') {
        format!("{path_value}{entry}")
    } else {
        format!("{path_value};{entry}")
    }
}

pub fn remove_path_entry(path_value: &str, entry: &Path) -> String {
    let wanted = normalized_path(entry);
    path_value
        .split(';')
        .filter(|item| normalized_path(Path::new(item)) != wanted)
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_matching_is_case_and_separator_insensitive() {
        let path = r"C:\Windows;C:\Users\ME\AppData\Local\Nalias\bin\";
        assert!(path_contains(
            path,
            Path::new(r"c:/users/me/appdata/local/nalias/bin")
        ));
    }

    #[test]
    fn add_does_not_duplicate() {
        let value = r"C:\A;C:\Nalias\bin";
        assert_eq!(add_path_entry(value, Path::new(r"c:\nalias\bin\")), value);
    }

    #[test]
    fn remove_preserves_other_entries() {
        assert_eq!(
            remove_path_entry(r"C:\A;C:\Nalias\bin;C:\B", Path::new(r"c:\nalias\bin")),
            r"C:\A;C:\B"
        );
    }
}
