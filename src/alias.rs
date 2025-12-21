use serde::{Deserialize, Serialize};

use crate::error::{NaliasError, Result};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    #[default]
    Cmd,
    Powershell,
    Direct,
}

impl std::fmt::Display for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cmd => f.write_str("cmd"),
            Self::Powershell => f.write_str("powershell"),
            Self::Direct => f.write_str("direct"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Alias {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub shell: Shell,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn default_true() -> bool {
    true
}

pub fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

pub fn validate_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let first = chars.next().ok_or_else(|| NaliasError::InvalidAliasName {
        name: name.to_owned(),
        reason: "the name cannot be empty".to_owned(),
    })?;
    if !first.is_ascii_alphabetic() {
        return Err(NaliasError::InvalidAliasName {
            name: name.to_owned(),
            reason: "the name must start with an ASCII letter".to_owned(),
        });
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(NaliasError::InvalidAliasName {
            name: name.to_owned(),
            reason: "only ASCII letters, digits, hyphens, and underscores are allowed".to_owned(),
        });
    }

    let lower = canonical_name(name);
    let reserved = matches!(lower.as_str(), "nalias" | "con" | "prn" | "aux" | "nul")
        || (lower.len() == 4
            && (lower.starts_with("com") || lower.starts_with("lpt"))
            && lower.as_bytes()[3].is_ascii_digit()
            && lower.as_bytes()[3] != b'0');
    if reserved {
        return Err(NaliasError::InvalidAliasName {
            name: name.to_owned(),
            reason: "the name is reserved by Nalias or Windows".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_names() {
        for valid in ["gs", "Git_Status", "serve-2"] {
            assert!(validate_name(valid).is_ok(), "{valid}");
        }
        for invalid in ["", "2bad", "../bad", "has space", "nalias", "CON", "com1"] {
            assert!(validate_name(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn canonicalizes_ascii_case() {
        assert_eq!(canonical_name("Git_Status"), "git_status");
    }
}
