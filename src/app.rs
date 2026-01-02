use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command as ProcessCommand;

use chrono::{SecondsFormat, Utc};

use crate::alias::{Alias, canonical_name, validate_name};
use crate::cli::{
    AddArgs, Cli, Command, EditArgs, InitArgs, JsonArgs, RemoveArgs, RenameArgs, RunArgs, ShowArgs,
    UninstallArgs,
};
use crate::config::{Config, atomic_write, reject_symlink};
use crate::error::{NaliasError, Result};
use crate::executor::{STACK_ENV, execute, plan, updated_stack};
use crate::paths::{AppPaths, add_path_entry, path_contains, remove_path_entry};
use crate::{platform, wrapper};

pub fn dispatch(cli: Cli) -> Result<i32> {
    let paths = AppPaths::resolve()?;
    let command = cli.command.unwrap_or(Command::Init(InitArgs {
        force: true,
        skip_path: false,
    }));
    match command {
        Command::Init(args) => init(&paths, args),
        Command::Add(args) => add(&paths, args),
        Command::Run(args) => run(&paths, args, cli.verbose),
        Command::List(args) => list(&paths, args),
        Command::Show(args) => show(&paths, args),
        Command::Edit(args) => edit(&paths, args),
        Command::Rename(args) => rename(&paths, args),
        Command::Remove(args) => remove(&paths, args),
        Command::Repair => {
            let summary = repair(&paths)?;
            println!(
                "Wrappers: {} created, {} repaired, {} unchanged, {} removed.",
                summary.created, summary.repaired, summary.unchanged, summary.removed
            );
            Ok(0)
        }
        Command::Doctor => doctor(&paths),
        Command::Uninstall(args) => uninstall(&paths, args),
    }
}

fn init(paths: &AppPaths, args: InitArgs) -> Result<i32> {
    reject_symlink(&paths.root, "Nalias directory")?;
    fs::create_dir_all(&paths.root)
        .map_err(|e| NaliasError::io("could not create the Nalias directory", e))?;
    reject_symlink(&paths.bin, "wrapper directory")?;
    fs::create_dir_all(&paths.bin)
        .map_err(|e| NaliasError::io("could not create the wrapper directory", e))?;

    if paths.config.exists() {
        Config::load(paths)?;
        println!("Configuration already exists: {}", paths.config.display());
    } else {
        Config::default().save(paths)?;
        println!("Created configuration: {}", paths.config.display());
    }

    let current = std::env::current_exe()
        .map_err(|e| NaliasError::io("could not locate the running executable", e))?;
    if same_path(&current, &paths.executable) {
        println!("Executable is already running from the install location.");
    } else if paths.executable.exists() && args.force && files_equal(&current, &paths.executable)? {
        println!(
            "Installed executable already matches this build: {}",
            paths.executable.display()
        );
    } else if !paths.executable.exists() || args.force {
        let bytes = fs::read(&current)
            .map_err(|e| NaliasError::io("could not read the running executable", e))?;
        atomic_write(&paths.executable, &bytes, false)
            .map_err(|e| NaliasError::Installation(format!("could not install nalias.exe: {e}")))?;
        println!("Installed executable: {}", paths.executable.display());
    } else {
        println!(
            "Kept existing executable: {} (use --force to replace it)",
            paths.executable.display()
        );
    }

    if args.skip_path {
        println!("Skipped user PATH modification.");
    } else if AppPaths::is_overridden() {
        println!("Skipped user PATH modification because NALIAS_HOME is set.");
    } else {
        let old = platform::user_path()?;
        let new = add_path_entry(&old, &paths.bin);
        if new == old {
            println!("Nalias bin directory is already on the user PATH.");
        } else {
            platform::set_user_path(&new)?;
            if let Err(error) = platform::broadcast_environment_change() {
                eprintln!("warning: {error}");
            }
            println!("Added {} to the current user's PATH.", paths.bin.display());
        }
    }
    println!("Nalias is initialized. Restart already-open terminals before using aliases.");
    Ok(0)
}

fn add(paths: &AppPaths, args: AddArgs) -> Result<i32> {
    validate_name(&args.name)?;
    if args.command.trim().is_empty() {
        return Err(NaliasError::Config(
            "alias command cannot be empty".to_owned(),
        ));
    }
    let (mut config, transaction) = Config::transaction(paths)?;
    let existing_key = config.find_key(&args.name).cloned();
    if existing_key.is_some() && !args.force {
        return Err(NaliasError::AliasExists(args.name));
    }
    warn_path_collision(paths, &args.name);
    let now = timestamp();
    let created_at = existing_key
        .as_ref()
        .and_then(|key| config.aliases.get(key))
        .map_or_else(|| now.clone(), |alias| alias.created_at.clone());
    if let Some(key) = existing_key {
        config.aliases.remove(&key);
    }
    let name = canonical_name(&args.name);
    config.aliases.insert(
        name.clone(),
        Alias {
            command: args.command,
            description: args.description,
            shell: args.shell,
            enabled: true,
            created_at,
            updated_at: now,
        },
    );

    let wrapper_existed = wrapper::wrapper_path(paths, &name)?.exists();
    wrapper::write(paths, &name)?;
    if let Err(error) = transaction.save(&config, paths) {
        if !wrapper_existed {
            let _ = wrapper::remove(paths, &name);
        }
        return Err(error);
    }
    println!("Alias '{name}' added.");
    Ok(0)
}

fn run(paths: &AppPaths, args: RunArgs, verbose: bool) -> Result<i32> {
    validate_name(&args.name)?;
    let config = Config::load(paths)?;
    let (name, alias) = config
        .get(&args.name)
        .ok_or_else(|| NaliasError::AliasNotFound(args.name.clone()))?;
    if !alias.enabled {
        return Err(NaliasError::AliasDisabled(name.to_owned()));
    }
    let old_stack = std::env::var(STACK_ENV).ok();
    let stack = updated_stack(name, old_stack.as_deref())?;
    if args.dry_run {
        let plan = plan(alias, &args.arguments)?;
        println!("shell: {}", alias.shell);
        println!("execute: {}", plan.display);
        return Ok(0);
    }
    execute(alias, &args.arguments, &stack, verbose)
}

fn list(paths: &AppPaths, args: JsonArgs) -> Result<i32> {
    let config = Config::load(paths)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&config.aliases).map_err(|e| {
                NaliasError::Config(format!("could not serialize aliases: {e}"))
            })?
        );
        return Ok(0);
    }
    if config.aliases.is_empty() {
        println!("No aliases defined.");
        return Ok(0);
    }
    let width = config
        .aliases
        .keys()
        .map(String::len)
        .max()
        .unwrap_or(4)
        .max(4);
    println!("{:<width$}  {:<10}  COMMAND", "NAME", "SHELL");
    for (name, alias) in &config.aliases {
        let disabled = if alias.enabled { "" } else { " [disabled]" };
        println!(
            "{name:<width$}  {:<10}  {}{disabled}",
            alias.shell, alias.command
        );
    }
    Ok(0)
}

fn show(paths: &AppPaths, args: ShowArgs) -> Result<i32> {
    validate_name(&args.name)?;
    let config = Config::load(paths)?;
    let (name, alias) = config
        .get(&args.name)
        .ok_or_else(|| NaliasError::AliasNotFound(args.name.clone()))?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(alias)
                .map_err(|e| { NaliasError::Config(format!("could not serialize alias: {e}")) })?
        );
    } else {
        println!("Name:        {name}");
        println!("Command:     {}", alias.command);
        println!(
            "Description: {}",
            alias.description.as_deref().unwrap_or("")
        );
        println!("Shell:       {}", alias.shell);
        println!("Enabled:     {}", alias.enabled);
        println!("Created:     {}", alias.created_at);
        println!("Updated:     {}", alias.updated_at);
    }
    Ok(0)
}

fn edit(paths: &AppPaths, args: EditArgs) -> Result<i32> {
    validate_name(&args.name)?;
    if args.command.is_none()
        && args.description.is_none()
        && args.shell.is_none()
        && !args.enable
        && !args.disable
    {
        return Err(NaliasError::Config(
            "edit requires at least one change option".to_owned(),
        ));
    }
    if args
        .command
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(NaliasError::Config(
            "alias command cannot be empty".to_owned(),
        ));
    }
    let (mut config, transaction) = Config::transaction(paths)?;
    let key = config
        .find_key(&args.name)
        .cloned()
        .ok_or_else(|| NaliasError::AliasNotFound(args.name.clone()))?;
    let alias = config.aliases.get_mut(&key).expect("key came from map");
    if let Some(command) = args.command {
        alias.command = command;
    }
    if let Some(description) = args.description {
        alias.description = Some(description);
    }
    if let Some(shell) = args.shell {
        alias.shell = shell;
    }
    if args.enable {
        alias.enabled = true;
    } else if args.disable {
        alias.enabled = false;
    }
    alias.updated_at = timestamp();
    transaction.save(&config, paths)?;
    println!("Alias '{key}' updated.");
    Ok(0)
}

fn rename(paths: &AppPaths, args: RenameArgs) -> Result<i32> {
    validate_name(&args.old_name)?;
    validate_name(&args.new_name)?;
    let (mut config, transaction) = Config::transaction(paths)?;
    let old_key = config
        .find_key(&args.old_name)
        .cloned()
        .ok_or_else(|| NaliasError::AliasNotFound(args.old_name.clone()))?;
    let new_key = canonical_name(&args.new_name);
    if old_key != new_key && config.find_key(&new_key).is_some() {
        return Err(NaliasError::AliasExists(args.new_name));
    }
    if old_key == new_key {
        println!("Alias is already named '{new_key}'.");
        return Ok(0);
    }
    let old_config = config.clone();
    let mut alias = config.aliases.remove(&old_key).expect("key came from map");
    alias.updated_at = timestamp();
    config.aliases.insert(new_key.clone(), alias);

    wrapper::write(paths, &new_key)?;
    if let Err(error) = transaction.save(&config, paths) {
        let _ = wrapper::remove(paths, &new_key);
        return Err(error);
    }
    if let Err(error) = wrapper::remove(paths, &old_key) {
        let _ = transaction.save(&old_config, paths);
        let _ = wrapper::remove(paths, &new_key);
        return Err(error);
    }
    println!("Alias '{old_key}' renamed to '{new_key}'.");
    Ok(0)
}

fn remove(paths: &AppPaths, args: RemoveArgs) -> Result<i32> {
    validate_name(&args.name)?;
    let (mut config, transaction) = Config::transaction(paths)?;
    let key = config
        .find_key(&args.name)
        .cloned()
        .ok_or_else(|| NaliasError::AliasNotFound(args.name.clone()))?;
    if !args.yes && !confirm(&format!("Remove alias '{key}'? [y/N] "))? {
        return Err(NaliasError::Cancelled);
    }
    let old_config = config.clone();
    config.aliases.remove(&key);
    transaction.save(&config, paths)?;
    if let Err(error) = wrapper::remove(paths, &key) {
        let _ = transaction.save(&old_config, paths);
        return Err(error);
    }
    println!("Alias '{key}' removed.");
    Ok(0)
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct RepairSummary {
    pub created: usize,
    pub repaired: usize,
    pub unchanged: usize,
    pub removed: usize,
}

pub fn repair(paths: &AppPaths) -> Result<RepairSummary> {
    let (config, _transaction) = Config::transaction(paths)?;
    fs::create_dir_all(&paths.bin)
        .map_err(|e| NaliasError::io("could not create the wrapper directory", e))?;
    let mut summary = RepairSummary::default();
    for name in config.aliases.keys() {
        match wrapper::state(paths, name)? {
            wrapper::WrapperState::Missing => {
                wrapper::write(paths, name)?;
                summary.created += 1;
            }
            wrapper::WrapperState::Mismatched => {
                wrapper::write(paths, name)?;
                summary.repaired += 1;
            }
            wrapper::WrapperState::Current => summary.unchanged += 1,
        }
    }

    let valid: BTreeSet<String> = config
        .aliases
        .keys()
        .map(|name| canonical_name(name))
        .collect();
    for entry in fs::read_dir(&paths.bin)
        .map_err(|e| NaliasError::io("could not inspect the wrapper directory", e))?
    {
        let entry = entry.map_err(|e| NaliasError::io("could not inspect wrapper entry", e))?;
        let path = entry.path();
        let is_cmd = path
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("cmd"));
        if !is_cmd || !wrapper::is_generated_file(&path)? {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|value| canonical_name(&value.to_string_lossy()))
            .unwrap_or_default();
        if !valid.contains(&stem) {
            fs::remove_file(&path)
                .map_err(|e| NaliasError::io("could not remove stale wrapper", e))?;
            summary.removed += 1;
        }
    }
    Ok(summary)
}

fn doctor(paths: &AppPaths) -> Result<i32> {
    let mut problems = 0usize;
    report(true, "LOCALAPPDATA or NALIAS_HOME is available", "");
    let root_ok = paths.root.is_dir();
    report(
        root_ok,
        "Nalias root directory",
        &paths.root.display().to_string(),
    );
    problems += usize::from(!root_ok);
    let exe_ok = paths.executable.is_file();
    report(
        exe_ok,
        "Installed executable",
        &paths.executable.display().to_string(),
    );
    problems += usize::from(!exe_ok);
    let bin_ok = paths.bin.is_dir();
    report(
        bin_ok,
        "Wrapper directory",
        &paths.bin.display().to_string(),
    );
    problems += usize::from(!bin_ok);

    let config = match Config::load(paths) {
        Ok(config) => {
            report(true, "Configuration is valid (version 1)", "");
            Some(config)
        }
        Err(error) => {
            report(false, "Configuration", &error.to_string());
            problems += 1;
            None
        }
    };
    if AppPaths::is_overridden() {
        println!("SKIP user PATH check (NALIAS_HOME is set)");
    } else {
        match platform::user_path() {
            Ok(value) => {
                let ok = path_contains(&value, &paths.bin);
                report(ok, "Wrapper directory is on the user PATH", "");
                problems += usize::from(!ok);
            }
            Err(error) => {
                report(false, "User PATH", &error.to_string());
                problems += 1;
            }
        }
    }
    if let Some(config) = config
        && bin_ok
    {
        let mut wrapper_problems = 0;
        for name in config.aliases.keys() {
            if wrapper::state(paths, name)? != wrapper::WrapperState::Current {
                wrapper_problems += 1;
            }
        }
        let valid: BTreeSet<String> = config
            .aliases
            .keys()
            .map(|name| canonical_name(name))
            .collect();
        for entry in fs::read_dir(&paths.bin)
            .map_err(|e| NaliasError::io("could not inspect wrapper directory", e))?
        {
            let path = entry
                .map_err(|e| NaliasError::io("could not inspect wrapper entry", e))?
                .path();
            if wrapper::is_generated_file(&path)? {
                let stem = path
                    .file_stem()
                    .map(|value| canonical_name(&value.to_string_lossy()))
                    .unwrap_or_default();
                if !valid.contains(&stem) {
                    wrapper_problems += 1;
                }
            }
        }
        report(
            wrapper_problems == 0,
            "Wrappers match aliases",
            &format!("{wrapper_problems} problem(s)"),
        );
        problems += wrapper_problems;
    }
    if exe_ok {
        let current = std::env::current_exe()
            .map_err(|e| NaliasError::io("could not locate the running executable", e))?;
        match files_equal(&current, &paths.executable) {
            Ok(equal) => {
                report(equal, "Installed executable matches this build", "");
                problems += usize::from(!equal);
            }
            Err(error) => {
                report(false, "Executable comparison", &error.to_string());
                problems += 1;
            }
        }
    }
    if problems == 0 {
        println!("Doctor found no important problems.");
        Ok(0)
    } else {
        println!("Doctor found {problems} important problem(s).");
        Ok(6)
    }
}

fn uninstall(paths: &AppPaths, args: UninstallArgs) -> Result<i32> {
    if !args.yes && !confirm("Uninstall Nalias? [y/N] ")? {
        return Err(NaliasError::Cancelled);
    }
    if !AppPaths::is_overridden() {
        let old = platform::user_path()?;
        let new = remove_path_entry(&old, &paths.bin);
        if new != old {
            platform::set_user_path(&new)?;
            if let Err(error) = platform::broadcast_environment_change() {
                eprintln!("warning: {error}");
            }
            println!("Removed the Nalias bin directory from the user PATH.");
        }
    }

    if paths.bin.is_dir() {
        for entry in fs::read_dir(&paths.bin)
            .map_err(|e| NaliasError::io("could not inspect wrapper directory", e))?
        {
            let path = entry
                .map_err(|e| NaliasError::io("could not inspect wrapper entry", e))?
                .path();
            if wrapper::is_generated_file(&path)? {
                fs::remove_file(&path)
                    .map_err(|e| NaliasError::io("could not remove generated wrapper", e))?;
            }
        }
        let _ = fs::remove_dir(&paths.bin);
    }
    if !args.keep_config {
        for path in [&paths.config, &paths.config.with_extension("json.bak")] {
            if path.exists() {
                fs::remove_file(path)
                    .map_err(|e| NaliasError::io("could not remove configuration", e))?;
            }
        }
    }

    let mut deferred = false;
    if paths.executable.exists() {
        match fs::remove_file(&paths.executable) {
            Ok(()) => {}
            Err(error)
                if same_path(
                    &std::env::current_exe().unwrap_or_default(),
                    &paths.executable,
                ) =>
            {
                platform::defer_delete(&paths.executable)?;
                deferred = true;
                eprintln!(
                    "nalias.exe is in use and was scheduled for deletion after this process exits ({error})."
                );
            }
            Err(error) => {
                return Err(NaliasError::io(
                    "could not remove installed executable",
                    error,
                ));
            }
        }
    }
    let _ = fs::remove_dir(&paths.root);
    if deferred {
        println!("Nalias was uninstalled; executable cleanup will finish shortly.");
    } else {
        println!("Nalias was uninstalled.");
    }
    if args.keep_config {
        println!("Configuration was kept at {}.", paths.config.display());
    }
    Ok(0)
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy()),
    }
}

fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    let left_metadata = fs::metadata(left)
        .map_err(|e| NaliasError::io("could not inspect current executable", e))?;
    let right_metadata = fs::metadata(right)
        .map_err(|e| NaliasError::io("could not inspect installed executable", e))?;
    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }
    let left =
        fs::read(left).map_err(|e| NaliasError::io("could not read current executable", e))?;
    let right =
        fs::read(right).map_err(|e| NaliasError::io("could not read installed executable", e))?;
    Ok(left == right)
}

fn warn_path_collision(paths: &AppPaths, name: &str) {
    let wrapper = wrapper::wrapper_path(paths, name).ok();
    let collision = ProcessCommand::new("where.exe")
        .arg(name)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|output| {
            output.lines().any(|line| {
                wrapper
                    .as_ref()
                    .is_none_or(|wrapper| !same_path(Path::new(line.trim()), wrapper))
            })
        });
    if collision {
        eprintln!(
            "warning: '{name}' already resolves to another command on PATH; PATH order determines which command runs"
        );
    }
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|e| NaliasError::io("could not display confirmation prompt", e))?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|e| NaliasError::io("could not read confirmation", e))?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn report(ok: bool, label: &str, details: &str) {
    let status = if ok { "OK  " } else { "FAIL" };
    if details.is_empty() {
        println!("{status} {label}");
    } else {
        println!("{status} {label}: {details}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::Shell;

    fn seeded(paths: &AppPaths) {
        fs::create_dir_all(&paths.bin).unwrap();
        let mut config = Config::default();
        config.aliases.insert(
            "old".to_owned(),
            Alias {
                command: "echo old".to_owned(),
                description: None,
                shell: Shell::Cmd,
                enabled: true,
                created_at: "2026-08-04T15:00:00Z".to_owned(),
                updated_at: "2026-08-04T15:00:00Z".to_owned(),
            },
        );
        config.save(paths).unwrap();
        wrapper::write(paths, "old").unwrap();
    }

    #[test]
    fn repair_creates_repairs_and_removes_only_generated_files() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("home"));
        seeded(&paths);
        fs::write(
            wrapper::wrapper_path(&paths, "old").unwrap(),
            format!("{0}\r\nbad", wrapper::MARKER),
        )
        .unwrap();
        fs::write(
            paths.bin.join("stale.cmd"),
            format!("{0}\r\n", wrapper::MARKER),
        )
        .unwrap();
        fs::write(paths.bin.join("mine.cmd"), "@echo off\r\necho mine").unwrap();
        let summary = repair(&paths).unwrap();
        assert_eq!(summary.repaired, 1);
        assert_eq!(summary.removed, 1);
        assert!(paths.bin.join("mine.cmd").exists());
    }

    #[test]
    fn rename_changes_config_and_wrappers() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("home"));
        seeded(&paths);
        rename(
            &paths,
            RenameArgs {
                old_name: "OLD".to_owned(),
                new_name: "new".to_owned(),
            },
        )
        .unwrap();
        let config = Config::load(&paths).unwrap();
        assert!(config.get("old").is_none());
        assert!(config.get("NEW").is_some());
        assert!(!paths.bin.join("old.cmd").exists());
        assert!(paths.bin.join("new.cmd").exists());
    }
}
