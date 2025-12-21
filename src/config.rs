use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::alias::{Alias, canonical_name, validate_name};
use crate::error::{NaliasError, Result};
use crate::paths::AppPaths;

pub const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Config {
    pub version: u32,
    pub aliases: BTreeMap<String, Alias>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            aliases: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn load(paths: &AppPaths) -> Result<Self> {
        if !paths.config.exists() {
            return Err(NaliasError::NotInitialized);
        }
        reject_symlink(&paths.config, "configuration")?;
        let text = fs::read_to_string(&paths.config)
            .map_err(|e| NaliasError::io("could not read aliases.json", e))?;
        let config: Self = serde_json::from_str(&text)
            .map_err(|e| NaliasError::Config(format!("aliases.json is malformed: {e}")))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != CONFIG_VERSION {
            return Err(NaliasError::Config(format!(
                "unsupported configuration version {} (this build supports version {CONFIG_VERSION})",
                self.version
            )));
        }
        let mut names = BTreeSet::new();
        for (name, alias) in &self.aliases {
            validate_name(name).map_err(|e| NaliasError::Config(e.to_string()))?;
            let canonical = canonical_name(name);
            if !names.insert(canonical) {
                return Err(NaliasError::Config(format!(
                    "aliases.json contains duplicate alias names differing only by case: '{name}'"
                )));
            }
            if alias.command.trim().is_empty() {
                return Err(NaliasError::Config(format!(
                    "alias '{name}' has an empty command"
                )));
            }
            validate_timestamp(name, "created_at", &alias.created_at)?;
            validate_timestamp(name, "updated_at", &alias.updated_at)?;
        }
        Ok(())
    }

    pub fn find_key(&self, name: &str) -> Option<&String> {
        self.aliases
            .keys()
            .find(|key| key.eq_ignore_ascii_case(name))
    }

    pub fn get(&self, name: &str) -> Option<(&str, &Alias)> {
        let key = self.find_key(name)?;
        Some((
            key.as_str(),
            self.aliases.get(key).expect("key came from map"),
        ))
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        fs::create_dir_all(&paths.root)
            .map_err(|e| NaliasError::io("could not create the Nalias directory", e))?;
        let _lock = ConfigLock::acquire(&paths.lock)?;
        self.save_unlocked(paths)
    }

    fn save_unlocked(&self, paths: &AppPaths) -> Result<()> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| NaliasError::Config(format!("could not serialize configuration: {e}")))?;
        bytes.push(b'\n');
        atomic_write(&paths.config, &bytes, true)
    }

    /// Locks the configuration before reading it. Keep the returned transaction
    /// alive through the complete mutation and use it to save the result.
    pub fn transaction(paths: &AppPaths) -> Result<(Self, ConfigTransaction)> {
        if !paths.config.exists() {
            return Err(NaliasError::NotInitialized);
        }
        let transaction = ConfigTransaction {
            _lock: ConfigLock::acquire(&paths.lock)?,
        };
        let config = Self::load(paths)?;
        Ok((config, transaction))
    }
}

pub struct ConfigTransaction {
    _lock: ConfigLock,
}

impl ConfigTransaction {
    pub fn save(&self, config: &Config, paths: &AppPaths) -> Result<()> {
        config.save_unlocked(paths)
    }
}

fn validate_timestamp(alias: &str, field: &str, value: &str) -> Result<()> {
    chrono::DateTime::parse_from_rfc3339(value).map_err(|e| {
        NaliasError::Config(format!(
            "alias '{alias}' has an invalid {field} timestamp: {e}"
        ))
    })?;
    Ok(())
}

pub fn reject_symlink(path: &Path, label: &str) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        let mut reparse = metadata.file_type().is_symlink();
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            reparse |= metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
        }
        if reparse {
            return Err(NaliasError::Config(format!(
                "refusing to use {label} reparse point '{}'",
                path.display()
            )));
        }
    }
    Ok(())
}

struct ConfigLock {
    path: PathBuf,
    _file: File,
}

impl ConfigLock {
    fn acquire(path: &Path) -> Result<Self> {
        for _ in 0..100 {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut file) => {
                    let _ = writeln!(file, "pid={}", std::process::id());
                    return Ok(Self {
                        path: path.to_owned(),
                        _file: file,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(error) => {
                    return Err(NaliasError::io(
                        "could not acquire the configuration lock",
                        error,
                    ));
                }
            }
        }
        Err(NaliasError::Config(
            "another Nalias process is updating aliases.json (lock timed out)".to_owned(),
        ))
    }
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn atomic_write(target: &Path, bytes: &[u8], keep_backup: bool) -> Result<()> {
    if target.exists() {
        reject_symlink(target, "destination")?;
    }
    let parent = target.parent().ok_or_else(|| {
        NaliasError::Config(format!("destination '{}' has no parent", target.display()))
    })?;
    reject_symlink(parent, "destination directory")?;
    fs::create_dir_all(parent)
        .map_err(|e| NaliasError::io("could not create destination directory", e))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = parent.join(format!(
        ".{}.{}.{}.tmp",
        target.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        nonce
    ));
    let write_result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| NaliasError::io("could not create temporary file", e))?;
        file.write_all(bytes)
            .map_err(|e| NaliasError::io("could not write temporary file", e))?;
        file.sync_all()
            .map_err(|e| NaliasError::io("could not flush temporary file", e))?;
        drop(file);
        replace_file(&temp, target, keep_backup)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result
}

#[cfg(windows)]
fn replace_file(temp: &Path, target: &Path, keep_backup: bool) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW};

    if !target.exists() {
        return fs::rename(temp, target)
            .map_err(|e| NaliasError::io("could not install the new file", e));
    }
    let backup = target.with_extension(format!(
        "{}.bak",
        target.extension().unwrap_or_default().to_string_lossy()
    ));
    let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let temp_wide: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let backup_wide: Vec<u16> = backup.as_os_str().encode_wide().chain(Some(0)).collect();
    let backup_ptr = if keep_backup {
        backup_wide.as_ptr()
    } else {
        std::ptr::null()
    };
    // SAFETY: All pointers refer to NUL-terminated UTF-16 buffers that remain alive for the call.
    let result = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            temp_wide.as_ptr(),
            backup_ptr,
            REPLACEFILE_IGNORE_MERGE_ERRORS,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        return Err(NaliasError::io(
            format!("could not atomically replace '{}'", target.display()),
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, target: &Path, keep_backup: bool) -> Result<()> {
    if target.exists() {
        if keep_backup {
            let backup = target.with_extension(format!(
                "{}.bak",
                target.extension().unwrap_or_default().to_string_lossy()
            ));
            fs::copy(target, backup)
                .map_err(|e| NaliasError::io("could not create configuration backup", e))?;
        }
        fs::remove_file(target)
            .map_err(|e| NaliasError::io("could not replace destination file", e))?;
    }
    fs::rename(temp, target).map_err(|e| NaliasError::io("could not install new file", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::Shell;

    fn alias() -> Alias {
        Alias {
            command: "git status".to_owned(),
            description: None,
            shell: Shell::Cmd,
            enabled: true,
            created_at: "2026-08-04T15:00:00Z".to_owned(),
            updated_at: "2026-08-04T15:00:00Z".to_owned(),
        }
    }

    #[test]
    fn serializes_and_loads_config() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("home"));
        let mut config = Config::default();
        config.aliases.insert("gs".to_owned(), alias());
        config.save(&paths).unwrap();
        assert_eq!(Config::load(&paths).unwrap(), config);
    }

    #[test]
    fn rejects_unsupported_version() {
        let config = Config {
            version: 99,
            aliases: BTreeMap::new(),
        };
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );
    }

    #[test]
    fn detects_case_insensitive_duplicates() {
        let mut config = Config::default();
        config.aliases.insert("gs".to_owned(), alias());
        config.aliases.insert("GS".to_owned(), alias());
        assert!(config.validate().is_err());
    }

    #[test]
    fn atomic_write_preserves_new_data_and_backup() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("data.json");
        atomic_write(&file, b"old", true).unwrap();
        atomic_write(&file, b"new", true).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert_eq!(fs::read(file.with_extension("json.bak")).unwrap(), b"old");
    }
}
