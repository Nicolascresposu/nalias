mod args;
mod paths;
mod platform;
mod wrapper;

use std::fs;
use std::io::{self, Write};

use args::Command;
use paths::AppPaths;

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("error: {error}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<(), String> {
    match args::parse()? {
        Command::Help => {
            print!("{}", args::HELP);
            Ok(())
        }
        Command::Version => {
            println!("nalias-lite {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        command => {
            let paths = AppPaths::resolve()?;
            match command {
                Command::Init { force, skip_path } => init(&paths, force, skip_path),
                Command::Add {
                    name,
                    command,
                    force,
                } => add(&paths, &name, &command, force),
                Command::List => list(&paths),
                Command::Remove { name, yes } => remove(&paths, &name, yes),
                Command::Help | Command::Version => unreachable!(),
            }
        }
    }
}

fn init(paths: &AppPaths, force: bool, skip_path: bool) -> Result<(), String> {
    paths.ensure_directories()?;
    let current = std::env::current_exe()
        .map_err(|error| format!("could not locate the running executable: {error}"))?;
    if paths.same_path(&current, &paths.executable) {
        println!("Executable is already running from the install location.");
    } else if paths.executable.exists()
        && force
        && paths.files_equal(&current, &paths.executable)?
    {
        println!("Installed executable already matches this build.");
    } else if !paths.executable.exists() || force {
        let bytes = fs::read(&current)
            .map_err(|error| format!("could not read the running executable: {error}"))?;
        wrapper::atomic_write(&paths.executable, &bytes)?;
        println!("Installed {}.", paths.executable.display());
    } else {
        println!("An executable is already installed; use init --force to replace it.");
    }

    if skip_path {
        println!("Skipped user PATH modification.");
    } else if AppPaths::is_overridden() {
        println!("Skipped user PATH modification because NALIAS_LITE_HOME is set.");
    } else {
        let old = platform::user_path()?;
        let new = paths::add_path_entry(&old, &paths.bin);
        if new == old {
            println!("The Nalias Lite bin directory is already on the user PATH.");
        } else {
            platform::set_user_path(&new)?;
            if let Err(error) = platform::broadcast_environment_change() {
                eprintln!("warning: {error}");
            }
            println!("Added {} to the user PATH.", paths.bin.display());
        }
    }
    println!("Nalias Lite is ready. Restart terminals that were already open.");
    Ok(())
}

fn add(paths: &AppPaths, name: &str, command: &str, force: bool) -> Result<(), String> {
    paths.ensure_directories()?;
    wrapper::validate_name(name)?;
    if command.is_empty() {
        return Err("the command cannot be empty".to_owned());
    }
    if command.contains(['\r', '\n']) {
        return Err("commands must fit on one line".to_owned());
    }
    let path = wrapper::path(paths, name)?;
    if path.exists() && !force {
        return Err(format!(
            "alias '{name}' already exists; use --force to replace it"
        ));
    }
    if path.exists() && !wrapper::is_generated(&path)? {
        return Err(format!(
            "refusing to replace unrelated file '{}'",
            path.display()
        ));
    }
    wrapper::write(paths, name, command)?;
    println!("Alias '{}' added.", name.to_ascii_lowercase());
    Ok(())
}

fn list(paths: &AppPaths) -> Result<(), String> {
    if !paths.bin.is_dir() {
        return Err("Nalias Lite is not initialized; run 'nalias-lite init'".to_owned());
    }
    let mut aliases = wrapper::read_all(paths)?;
    if aliases.is_empty() {
        println!("No aliases defined.");
        return Ok(());
    }
    aliases.sort_by(|left, right| left.0.cmp(&right.0));
    let width = aliases
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    println!("{:<width$}  COMMAND", "NAME");
    for (name, command) in aliases {
        println!("{name:<width$}  {command}");
    }
    Ok(())
}

fn remove(paths: &AppPaths, name: &str, yes: bool) -> Result<(), String> {
    wrapper::validate_name(name)?;
    let path = wrapper::path(paths, name)?;
    if !path.exists() {
        return Err(format!("alias '{name}' was not found"));
    }
    if !wrapper::is_generated(&path)? {
        return Err(format!(
            "refusing to remove unrelated file '{}'",
            path.display()
        ));
    }
    if !yes && !confirm(&format!("Remove alias '{name}'? [y/N] "))? {
        return Err("operation cancelled".to_owned());
    }
    fs::remove_file(&path)
        .map_err(|error| format!("could not remove '{}': {error}", path.display()))?;
    println!("Alias '{}' removed.", name.to_ascii_lowercase());
    Ok(())
}

fn confirm(prompt: &str) -> Result<bool, String> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|error| format!("could not display confirmation: {error}"))?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("could not read confirmation: {error}"))?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
