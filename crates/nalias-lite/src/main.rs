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

fn run() -> Result<(), &'static str> {
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
                Command::Show => show(&paths),
                Command::Remove { name, yes } => remove(&paths, &name, yes),
                Command::Help | Command::Version => unreachable!(),
            }
        }
    }
}

fn init(paths: &AppPaths, force: bool, skip_path: bool) -> Result<(), &'static str> {
    paths.ensure_directories()?;
    let current = std::env::current_exe().map_err(|_| "could not locate this executable")?;
    if paths.same_path(&current, &paths.executable) {
        println!("Executable is already running from the install location.");
    } else if paths.executable.exists()
        && force
        && paths.files_equal(&current, &paths.executable)?
    {
        println!("Installed executable already matches this build.");
    } else if !paths.executable.exists() || force {
        let bytes = fs::read(current).map_err(|_| "could not read this executable")?;
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

fn add(paths: &AppPaths, name: &str, command: &str, force: bool) -> Result<(), &'static str> {
    paths.ensure_directories()?;
    wrapper::validate_name(name)?;
    if command.is_empty() {
        return Err("the command cannot be empty");
    }
    if command.contains(['\r', '\n']) {
        return Err("commands must fit on one line");
    }
    let path = wrapper::path(paths, name)?;
    if path.exists() && !force {
        return Err("alias already exists; use --force to replace it");
    }
    if path.exists() && !wrapper::is_generated(&path)? {
        return Err("refusing to replace an unrelated file");
    }
    wrapper::write(paths, name, command)?;
    println!("Alias '{}' added.", name.to_ascii_lowercase());
    Ok(())
}

fn show(paths: &AppPaths) -> Result<(), &'static str> {
    paths.ensure_directories()?;
    platform::open_folder(&paths.bin)?;
    println!("Opened {}.", paths.bin.display());
    Ok(())
}

fn remove(paths: &AppPaths, name: &str, yes: bool) -> Result<(), &'static str> {
    wrapper::validate_name(name)?;
    let path = wrapper::path(paths, name)?;
    if !path.exists() {
        return Err("alias was not found");
    }
    if !wrapper::is_generated(&path)? {
        return Err("refusing to remove an unrelated file");
    }
    if !yes && !confirm("Remove this alias? [y/N] ")? {
        return Err("operation cancelled");
    }
    fs::remove_file(path).map_err(|_| "could not remove the alias")?;
    println!("Alias '{}' removed.", name.to_ascii_lowercase());
    Ok(())
}

fn confirm(prompt: &str) -> Result<bool, &'static str> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|_| "could not display confirmation")?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|_| "could not read confirmation")?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
