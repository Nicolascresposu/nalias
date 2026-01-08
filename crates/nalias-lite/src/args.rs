pub const HELP: &str = "Nalias Lite - minimal direct-wrapper aliases for Windows\n\nUsage:\n  nalias-lite [COMMAND]\n\nCommands:\n  init [--force] [--skip-path]       Install Nalias Lite\n  add <name> <command> [--force]     Create a direct .cmd alias\n  list                               List managed aliases\n  remove <name> [--yes]              Remove an alias\n  help                               Print this help\n\nRunning without arguments is equivalent to 'init --force'.\n";

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
    List,
    Remove {
        name: String,
        yes: bool,
    },
    Help,
    Version,
}

pub fn parse() -> Result<Command, String> {
    parse_values(std::env::args().skip(1).collect())
}

fn parse_values(values: Vec<String>) -> Result<Command, String> {
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
            let options = &values[1..];
            reject_unknown(options, &["--force", "--skip-path"])?;
            Ok(Command::Init {
                force: options.iter().any(|value| value == "--force"),
                skip_path: options.iter().any(|value| value == "--skip-path"),
            })
        }
        "add" => {
            if values.len() < 3 {
                return Err("usage: nalias-lite add <name> <command> [--force]".to_owned());
            }
            reject_unknown(&values[3..], &["--force"])?;
            Ok(Command::Add {
                name: values[1].clone(),
                command: values[2].clone(),
                force: values[3..].iter().any(|value| value == "--force"),
            })
        }
        "list" => exact_len(&values, 1).map(|()| Command::List),
        "remove" => {
            if values.len() < 2 {
                return Err("usage: nalias-lite remove <name> [--yes]".to_owned());
            }
            reject_unknown(&values[2..], &["--yes", "-y"])?;
            Ok(Command::Remove {
                name: values[1].clone(),
                yes: values[2..]
                    .iter()
                    .any(|value| value == "--yes" || value == "-y"),
            })
        }
        unknown => Err(format!("unknown command '{unknown}'\n\n{HELP}")),
    }
}

fn exact_len(values: &[String], length: usize) -> Result<(), String> {
    if values.len() == length {
        Ok(())
    } else {
        Err("too many arguments".to_owned())
    }
}

fn reject_unknown(values: &[String], allowed: &[&str]) -> Result<(), String> {
    if let Some(value) = values
        .iter()
        .find(|value| !allowed.contains(&value.as_str()))
    {
        Err(format!("unknown option '{value}'"))
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
    fn no_arguments_force_initializes() {
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
        assert_eq!(parse_values(values(&["list"])).unwrap(), Command::List);
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
