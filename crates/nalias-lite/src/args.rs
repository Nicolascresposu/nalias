pub const HELP: &str = "Nalias Lite - minimal direct-wrapper aliases for Windows\n\nUsage:\n  nalias-lite [COMMAND]\n\nCommands:\n  init [--force] [--skip-path]       Install Nalias Lite\n  add <name> <command> [--force]     Create a direct .cmd alias\n  show                               Open the alias folder\n  remove <name> [--yes]              Remove an alias\n  help                               Print this help\n\nRunning without arguments is equivalent to 'init --force'.\n";

#[derive(Debug, Eq, PartialEq)]
pub enum Command {
    Init {
        force: bool,
        skip_path: bool,
    },
    Add {
        name: String,
        command: String,
        force: bool,
    },
    Show,
    Remove {
        name: String,
        yes: bool,
    },
    Help,
    Version,
}

pub fn parse() -> Result<Command, &'static str> {
    parse_values(std::env::args().skip(1).collect())
}

fn parse_values(values: Vec<String>) -> Result<Command, &'static str> {
    if values.is_empty() {
        return Ok(Command::Init {
            force: true,
            skip_path: false,
        });
    }
    match values[0].as_str() {
        "help" | "--help" | "-h" => exact_len(&values, 1).map(|()| Command::Help),
        "--version" | "-V" => exact_len(&values, 1).map(|()| Command::Version),
        "init" => {
            reject_unknown(&values[1..], &["--force", "--skip-path"])?;
            Ok(Command::Init {
                force: values[1..].iter().any(|value| value == "--force"),
                skip_path: values[1..].iter().any(|value| value == "--skip-path"),
            })
        }
        "show" => exact_len(&values, 1).map(|()| Command::Show),
        "add" => {
            if values.len() < 3 {
                return Err("usage: nalias-lite add <name> <command> [--force]");
            }
            reject_unknown(&values[3..], &["--force"])?;
            Ok(Command::Add {
                name: values[1].clone(),
                command: values[2].clone(),
                force: values[3..].iter().any(|value| value == "--force"),
            })
        }
        "remove" => {
            if values.len() < 2 {
                return Err("usage: nalias-lite remove <name> [--yes]");
            }
            reject_unknown(&values[2..], &["--yes", "-y"])?;
            Ok(Command::Remove {
                name: values[1].clone(),
                yes: values[2..]
                    .iter()
                    .any(|value| value == "--yes" || value == "-y"),
            })
        }
        _ => Err("unknown command; run 'nalias-lite help'"),
    }
}

fn exact_len(values: &[String], length: usize) -> Result<(), &'static str> {
    if values.len() == length {
        Ok(())
    } else {
        Err("too many arguments")
    }
}

fn reject_unknown(values: &[String], allowed: &[&str]) -> Result<(), &'static str> {
    if values
        .iter()
        .any(|value| !allowed.contains(&value.as_str()))
    {
        Err("unknown option")
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn no_arguments_force_initialize() {
        assert_eq!(
            parse_values(Vec::new()).unwrap(),
            Command::Init {
                force: true,
                skip_path: false
            }
        );
    }

    #[test]
    fn parses_minimal_commands() {
        assert_eq!(parse_values(values(&["show"])).unwrap(), Command::Show);
        assert_eq!(
            parse_values(values(&["add", "gs", "git status", "--force"])).unwrap(),
            Command::Add {
                name: "gs".to_owned(),
                command: "git status".to_owned(),
                force: true
            }
        );
    }
}
