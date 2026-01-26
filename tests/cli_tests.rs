#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn nalias(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nalias"))
        .args(args)
        .env("NALIAS_HOME", home)
        .output()
        .expect("nalias should start")
}

fn succeed(home: &Path, args: &[&str]) -> Output {
    let output = nalias(home, args);
    assert!(
        output.status.success(),
        "command {args:?} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_wrapper(home: &Path, name: &str, arguments: &[&str]) -> Output {
    let mut command = Command::new("cmd.exe");
    command
        .args(["/D", "/S", "/C"])
        .arg(name)
        .args(arguments)
        .env("NALIAS_HOME", home);
    prepend_path(&mut command, home.join("bin"));
    command.output().expect("wrapper should start")
}

fn prepend_path(command: &mut Command, entry: PathBuf) {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut value = entry.into_os_string();
    value.push(";");
    value.push(current);
    command.env("PATH", value);
}

#[test]
fn complete_alias_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("Nalias Test");

    succeed(&home, &["init", "--skip-path"]);
    succeed(&home, &["add", "echo-test", "echo hello"]);

    let output = run_wrapper(&home, "echo-test", &[]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));

    let output = succeed(&home, &["list"]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("echo-test"));
    let output = succeed(&home, &["show", "ECHO-TEST", "--json"]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("echo hello"));

    succeed(&home, &["edit", "echo-test", "--command", "echo changed"]);
    let output = run_wrapper(&home, "echo-test", &[]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("changed"));

    succeed(&home, &["rename", "echo-test", "renamed-test"]);
    assert!(!home.join("bin").join("echo-test.cmd").exists());
    let output = run_wrapper(&home, "renamed-test", &[]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("changed"));

    succeed(&home, &["remove", "renamed-test", "--yes"]);
    assert!(!home.join("bin").join("renamed-test.cmd").exists());
}

#[test]
fn launching_without_arguments_initializes_and_force_updates() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");

    succeed(&home, &[]);
    let installed = home.join("nalias.exe");
    assert!(installed.is_file());
    assert!(home.join("aliases.json").is_file());
    assert!(home.join("bin").is_dir());

    let output = succeed(&home, &[]);
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("Installed executable already matches this build")
    );

    succeed(&home, &["add", "preserved", "echo preserved"]);
    std::fs::write(&installed, b"old executable").unwrap();
    let output = succeed(&home, &[]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Installed executable:"));

    assert!(std::fs::metadata(&installed).unwrap().len() > 1024);
    assert!(succeed(&home, &["show", "preserved"]).status.success());
}

#[test]
fn forwards_arguments_and_supports_dry_run() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    succeed(&home, &["init", "--skip-path"]);
    succeed(&home, &["add", "say", "echo"]);
    let output = run_wrapper(&home, "say", &["two words", "100%", "x& exit 42"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("two words"), "{stdout}");
    assert!(stdout.contains("100%"), "{stdout}");
    assert!(stdout.contains("x& exit 42"), "{stdout}");

    // Launch Nalias directly so the values reach its cmd-quoting layer without
    // first being parsed by the outer wrapper shell.
    let dangerous = r#"q" & exit 42 | echo injected > nul < nul (paren) ^ %PATH% !bang!"#;
    let output = nalias(&home, &["run", "say", dangerous]);
    assert!(
        output.status.success(),
        "forwarded metacharacters escaped the argument: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("exit 42"), "{stdout}");
    assert!(stdout.contains("%PATH%"), "{stdout}");
    assert!(stdout.contains("!bang!"), "{stdout}");

    let probe = temp.path().join("argument probe.ps1");
    std::fs::write(
        &probe,
        r#"[Console]::WriteLine("COUNT={0}", $args.Count)
foreach ($Value in $args) { [Console]::WriteLine("ARG=<{0}>", $Value) }
"#,
    )
    .unwrap();
    let powershell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let probe_command = format!(
        "\"{}\" -NoLogo -NoProfile -File \"{}\"",
        powershell.display(),
        probe.display()
    );
    succeed(&home, &["add", "argument-probe", probe_command.as_str()]);
    let output = run_wrapper(&home, "argument-probe", &["first commit"]);
    assert!(
        output.status.success(),
        "argument probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("COUNT=1"), "{stdout}");
    assert!(stdout.contains("ARG=<first commit>"), "{stdout}");

    let output = succeed(&home, &["run", "say", "--dry-run"]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("execute:"));
}

#[test]
fn direct_mode_returns_the_child_exit_code_and_init_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    succeed(&home, &["init", "--skip-path"]);
    succeed(&home, &["add", "keepme", "echo preserved"]);
    succeed(&home, &["init", "--skip-path"]);
    assert!(succeed(&home, &["show", "keepme"]).status.success());

    succeed(
        &home,
        &[
            "add",
            "direct-exit",
            "cmd.exe /D /C exit 7",
            "--shell",
            "direct",
        ],
    );
    let output = nalias(&home, &["run", "direct-exit"]);
    assert_eq!(output.status.code(), Some(7));
    assert!(succeed(&home, &["doctor"]).status.success());
}

#[test]
fn detects_recursion_through_generated_wrappers() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    succeed(&home, &["init", "--skip-path"]);
    succeed(&home, &["add", "loopalias", "loopalias"]);
    let output = run_wrapper(&home, "loopalias", &[]);
    assert_eq!(output.status.code(), Some(5));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("recursion detected"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn refuses_to_reset_malformed_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    succeed(&home, &["init", "--skip-path"]);
    let config = home.join("aliases.json");
    std::fs::write(&config, "not json").unwrap();
    let output = nalias(&home, &["init", "--force", "--skip-path"]);
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(std::fs::read_to_string(config).unwrap(), "not json");
}

#[test]
fn uninstall_preserves_config_and_unrelated_files_when_requested() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    succeed(&home, &["init", "--skip-path"]);
    succeed(&home, &["add", "temporary", "echo temporary"]);
    let unrelated = home.join("bin").join("mine.cmd");
    std::fs::write(&unrelated, "@echo off\r\necho user-owned\r\n").unwrap();

    succeed(&home, &["uninstall", "--keep-config", "--yes"]);
    assert!(home.join("aliases.json").exists());
    assert!(unrelated.exists());
    assert!(!home.join("bin").join("temporary.cmd").exists());
    assert!(!home.join("nalias.exe").exists());
}
