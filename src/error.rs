use std::io;

#[derive(Debug, thiserror::Error)]
pub enum NaliasError {
    #[error("Nalias is not initialized")]
    NotInitialized,
    #[error("alias '{0}' was not found")]
    AliasNotFound(String),
    #[error("alias '{0}' already exists")]
    AliasExists(String),
    #[error("invalid alias name '{name}': {reason}")]
    InvalidAliasName { name: String, reason: String },
    #[error("alias '{0}' is disabled")]
    AliasDisabled(String),
    #[error("alias recursion detected: {0}")]
    Recursion(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("installation error: {0}")]
    Installation(String),
    #[error("could not execute alias: {0}")]
    Execution(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
}

pub type Result<T> = std::result::Result<T, NaliasError>;

impl NaliasError {
    pub fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::AliasNotFound(_) => 3,
            Self::Config(_) => 4,
            Self::Execution(_) | Self::Recursion(_) | Self::AliasDisabled(_) => 5,
            Self::Installation(_) | Self::NotInitialized => 6,
            _ => 1,
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::NotInitialized => Some("run 'nalias init'"),
            Self::AliasExists(_) => Some("use --force to replace it"),
            Self::AliasNotFound(_) => Some("run 'nalias list' to see available aliases"),
            _ => None,
        }
    }
}
