use clap::{Args, Parser, Subcommand};

use crate::alias::Shell;

#[derive(Debug, Parser)]
#[command(
    name = "nalias",
    version,
    about = "Nalias — lightweight persistent command aliases for Windows",
    propagate_version = true
)]
pub struct Cli {
    /// Print execution diagnostics.
    #[arg(long, global = true)]
    pub verbose: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install and initialize Nalias.
    Init(InitArgs),
    /// Create an alias.
    Add(AddArgs),
    /// Execute an alias.
    Run(RunArgs),
    /// List aliases.
    List(JsonArgs),
    /// Show alias details.
    Show(ShowArgs),
    /// Modify an alias.
    Edit(EditArgs),
    /// Rename an alias.
    Rename(RenameArgs),
    /// Delete an alias.
    Remove(RemoveArgs),
    /// Repair generated wrappers.
    Repair,
    /// Diagnose the installation.
    Doctor,
    /// Remove Nalias.
    Uninstall(UninstallArgs),
}

#[derive(Debug, Args, Default)]
pub struct InitArgs {
    /// Reinstall the executable even if one is already installed.
    #[arg(long)]
    pub force: bool,
    /// Do not modify the current user's PATH.
    #[arg(long)]
    pub skip_path: bool,
}

#[derive(Debug, Args)]
pub struct AddArgs {
    pub name: String,
    pub command: String,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long, value_enum, default_value_t = Shell::Cmd)]
    pub shell: Shell,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    pub name: String,
    /// Print the resolved invocation without running it.
    #[arg(long)]
    pub dry_run: bool,
    /// Arguments forwarded to the alias.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub arguments: Vec<String>,
}

#[derive(Debug, Args)]
pub struct JsonArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    pub name: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct EditArgs {
    pub name: String,
    #[arg(long)]
    pub command: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long, value_enum)]
    pub shell: Option<Shell>,
    #[arg(long, conflicts_with = "disable")]
    pub enable: bool,
    #[arg(long, conflicts_with = "enable")]
    pub disable: bool,
}

#[derive(Debug, Args)]
pub struct RenameArgs {
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    pub name: String,
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    #[arg(long)]
    pub keep_config: bool,
    #[arg(long, short = 'y')]
    pub yes: bool,
}
