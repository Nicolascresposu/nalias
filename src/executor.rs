use std::process::Command;

use crate::alias::{Alias, Shell, canonical_name};
use crate::error::{NaliasError, Result};

pub const STACK_ENV: &str = "NALIAS_ALIAS_STACK";
pub const MAX_NESTING: usize = 32;
const STACK_SEPARATOR: char = ';';

#[derive(Debug, Eq, PartialEq)]
pub struct ExecutionPlan {
    pub program: String,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub display: String,
}

pub fn updated_stack(name: &str, current: Option<&str>) -> Result<String> {
    let canonical = canonical_name(name);
    let mut stack: Vec<String> = current
        .unwrap_or_default()
        .split(STACK_SEPARATOR)
        .filter(|item| !item.is_empty())
        .map(canonical_name)
        .collect();
    if let Some(position) = stack.iter().position(|item| item == &canonical) {
        let mut cycle = stack[position..].to_vec();
        cycle.push(canonical);
        return Err(NaliasError::Recursion(cycle.join(" -> ")));
    }
    if stack.len() >= MAX_NESTING {
        return Err(NaliasError::Recursion(format!(
            "maximum nesting depth of {MAX_NESTING} was exceeded"
        )));
    }
    stack.push(canonical);
    Ok(stack.join(&STACK_SEPARATOR.to_string()))
}

pub fn plan(alias: &Alias, forwarded: &[String]) -> Result<ExecutionPlan> {
    match alias.shell {
        Shell::Cmd => {
            let (command_line, environment) = cmd_invocation(&alias.command, forwarded);
            let logical = append_cmd_arguments(&alias.command, forwarded);
            Ok(ExecutionPlan {
                program: "cmd.exe".to_owned(),
                arguments: vec![
                    "/D".to_owned(),
                    "/S".to_owned(),
                    "/V:ON".to_owned(),
                    "/C".to_owned(),
                    command_line,
                ],
                environment,
                display: format!("cmd.exe /D /S /V:ON /C {logical}"),
            })
        }
        Shell::Powershell => {
            let command_line = append_powershell_arguments(&alias.command, forwarded);
            Ok(ExecutionPlan {
                program: "powershell.exe".to_owned(),
                arguments: vec![
                    "-NoLogo".to_owned(),
                    "-NoProfile".to_owned(),
                    "-Command".to_owned(),
                    command_line.clone(),
                ],
                environment: Vec::new(),
                display: format!("powershell.exe -NoLogo -NoProfile -Command {command_line}"),
            })
        }
        Shell::Direct => {
            let mut words = split_windows_command_line(&alias.command)?;
            if words.is_empty() {
                return Err(NaliasError::Execution(
                    "direct alias command does not contain a program".to_owned(),
                ));
            }
            let program = words.remove(0);
            words.extend_from_slice(forwarded);
            let display = std::iter::once(program.as_str())
                .chain(words.iter().map(String::as_str))
                .map(quote_for_display)
                .collect::<Vec<_>>()
                .join(" ");
            Ok(ExecutionPlan {
                program,
                arguments: words,
                environment: Vec::new(),
                display,
            })
        }
    }
}

pub fn execute(alias: &Alias, forwarded: &[String], stack: &str, verbose: bool) -> Result<i32> {
    let plan = plan(alias, forwarded)?;
    if verbose {
        eprintln!("nalias: executing [{}] {}", alias.shell, plan.display);
    }
    let status = Command::new(&plan.program)
        .args(&plan.arguments)
        .envs(plan.environment.iter().map(|(key, value)| (key, value)))
        .env(STACK_ENV, stack)
        .status()
        .map_err(|e| NaliasError::Execution(format!("failed to start '{}': {e}", plan.program)))?;
    Ok(status.code().unwrap_or(5))
}

/// Builds the actual CMD invocation. Forwarded data is carried in environment
/// variables and expanded with delayed expansion. CMD recognizes metacharacters
/// before this late expansion, so argument data cannot become shell syntax.
fn cmd_invocation(command: &str, arguments: &[String]) -> (String, Vec<(String, String)>) {
    let mut command_line = command.to_owned();
    let mut environment = Vec::with_capacity(arguments.len());
    for (index, value) in arguments.iter().enumerate() {
        let name = format!("NALIAS__FORWARDED_{index}");
        command_line.push_str(" \"!");
        command_line.push_str(&name);
        command_line.push_str("!\"");
        environment.push((name, value.clone()));
    }
    (command_line, environment)
}

pub fn append_cmd_arguments(command: &str, arguments: &[String]) -> String {
    let mut result = command.to_owned();
    for argument in arguments {
        result.push(' ');
        result.push_str(&quote_cmd_argument(argument));
    }
    result
}

/// Quotes a forwarded value for logical display and tests. Actual CMD execution
/// uses delayed-expansion placeholders because no early-expansion quoting scheme
/// can safely represent every listed metacharacter.
pub fn quote_cmd_argument(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    let mut trailing_backslashes = 0;
    for ch in value.chars() {
        match ch {
            '\\' => {
                trailing_backslashes += 1;
                result.push('\\');
            }
            '"' => {
                for _ in 0..trailing_backslashes {
                    result.push('\\');
                }
                trailing_backslashes = 0;
                result.push_str("\"\"");
            }
            '%' => {
                trailing_backslashes = 0;
                result.push_str("%%");
            }
            _ => {
                trailing_backslashes = 0;
                result.push(ch);
            }
        }
    }
    for _ in 0..trailing_backslashes {
        result.push('\\');
    }
    result.push('"');
    result
}

pub fn append_powershell_arguments(command: &str, arguments: &[String]) -> String {
    let mut result = command.to_owned();
    for argument in arguments {
        result.push(' ');
        result.push('\'');
        result.push_str(&argument.replace('\'', "''"));
        result.push('\'');
    }
    result
}

/// Parses the stored direct command using the Microsoft C command-line rules.
pub fn split_windows_command_line(input: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i == chars.len() {
            break;
        }
        let mut word = String::new();
        let mut quoted = false;
        while i < chars.len() && (quoted || !chars[i].is_whitespace()) {
            if chars[i] == '\\' {
                let start = i;
                while i < chars.len() && chars[i] == '\\' {
                    i += 1;
                }
                let count = i - start;
                if i < chars.len() && chars[i] == '"' {
                    word.extend(std::iter::repeat_n('\\', count / 2));
                    if count % 2 == 0 {
                        quoted = !quoted;
                    } else {
                        word.push('"');
                    }
                    i += 1;
                } else {
                    word.extend(std::iter::repeat_n('\\', count));
                }
            } else if chars[i] == '"' {
                quoted = !quoted;
                i += 1;
            } else {
                word.push(chars[i]);
                i += 1;
            }
        }
        if quoted {
            return Err(NaliasError::Execution(
                "direct alias command contains an unterminated quote".to_owned(),
            ));
        }
        result.push(word);
    }
    Ok(result)
}

fn quote_for_display(value: &str) -> String {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_cmd_metacharacters_and_percent() {
        assert_eq!(quote_cmd_argument("a & b|c>(d)^e"), r#""a & b|c>(d)^e""#);
        assert_eq!(quote_cmd_argument("%PATH%"), r#""%%PATH%%""#);
        assert_eq!(quote_cmd_argument("a\"b"), r#""a""b""#);
        assert_eq!(quote_cmd_argument(""), r#""""#);
    }

    #[test]
    fn cmd_invocation_uses_late_expansion_placeholders() {
        let values = vec![r#"a" & exit 42 | %PATH%"#.to_owned()];
        let (line, environment) = cmd_invocation("echo", &values);
        assert_eq!(line, r#"echo "!NALIAS__FORWARDED_0!""#);
        assert_eq!(
            environment,
            vec![("NALIAS__FORWARDED_0".to_owned(), values[0].clone())]
        );
    }

    #[test]
    fn preserves_trailing_backslashes_for_cmd() {
        assert_eq!(quote_cmd_argument(r"C:\A\"), r#""C:\A\\""#);
    }

    #[test]
    fn powershell_uses_literal_single_quotes() {
        assert_eq!(
            append_powershell_arguments("Write-Output", &["a'b&c".to_owned()]),
            "Write-Output 'a''b&c'"
        );
    }

    #[test]
    fn direct_parser_handles_quotes_and_backslashes() {
        assert_eq!(
            split_windows_command_line(r#"python -m "http server" "C:\a b""#).unwrap(),
            ["python", "-m", "http server", r"C:\a b"]
        );
        assert!(split_windows_command_line(r#"python "oops"#).is_err());
    }

    #[test]
    fn detects_direct_and_indirect_recursion() {
        assert!(updated_stack("a", Some("a")).is_err());
        let error = updated_stack("a", Some("a;b")).unwrap_err();
        assert!(error.to_string().contains("a -> b -> a"));
        assert_eq!(updated_stack("b", Some("a")).unwrap(), "a;b");
    }

    #[test]
    fn limits_nesting() {
        let stack = (0..MAX_NESTING)
            .map(|index| format!("a{index}"))
            .collect::<Vec<_>>()
            .join(";");
        assert!(updated_stack("next", Some(&stack)).is_err());
    }
}
