pub mod alias;
pub mod app;
pub mod cli;
pub mod config;
pub mod error;
pub mod executor;
pub mod paths;
pub mod platform;
pub mod wrapper;

pub use app::dispatch;
pub use cli::Cli;
pub use error::{NaliasError, Result};
