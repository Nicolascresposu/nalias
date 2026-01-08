#![cfg(windows)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct TestHome(PathBuf);

impl TestHome {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "nalias-lite-integration-{}-{nonce}",
            std::process::id()
        )))
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn lite(home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nalias-lite"))
        .args(arguments)
        .env("NALIAS_LITE_HOME", home)
        .output()
        .expect("nalias-lite should start")
}

fn succeed(home: &Path, arguments: &[&str]) -> Output {
    let output = lite(home, arguments);
    assert!(
        output.status.success(),
        "command {arguments:?} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn direct_wrapper_lifecycle() {
    let home = TestHome::new();
    succeed(&home.0, &[]);
    succeed(&home.0, &["add", "hello", "echo hello"]);

    let wrapper = home.0.join("bin").join("hello.cmd");
    let body = std::fs::read_to_string(&wrapper).unwrap();
    assert!(body.contains("echo hello %*"));
    assert!(!body.contains("nalias-lite.exe run"));

    let mut path = OsString::from(home.0.join("bin"));
    path.push(";");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    let output = Command::new("cmd.exe")
        .args(["/D", "/C", "hello", "world"])
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("hello world"));

    let output = succeed(&home.0, &["list"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello"));
    assert!(stdout.contains("echo hello"));

    succeed(&home.0, &["add", "hello", "echo changed", "--force"]);
    assert!(
        std::fs::read_to_string(&wrapper)
            .unwrap()
            .contains("echo changed %*")
    );
    succeed(&home.0, &["remove", "HELLO", "--yes"]);
    assert!(!wrapper.exists());
}

#[test]
fn never_overwrites_or_deletes_unrelated_wrappers() {
    let home = TestHome::new();
    succeed(&home.0, &[]);
    let unrelated = home.0.join("bin").join("mine.cmd");
    std::fs::write(&unrelated, "@echo off\r\necho user-owned\r\n").unwrap();

    assert!(
        !lite(&home.0, &["add", "mine", "echo replaced", "--force"])
            .status
            .success()
    );
    assert!(!lite(&home.0, &["remove", "mine", "--yes"]).status.success());
    assert!(unrelated.exists());
}
