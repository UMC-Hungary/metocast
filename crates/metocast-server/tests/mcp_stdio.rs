//! `metocast-server mcp-stdio` runs the stdio bridge, not the server. The server starts embedded
//! PostgreSQL on a fixed port, and a failed start kills whatever listens there, so the
//! subcommand must be dispatched before any of that. These run the real binary with an empty
//! environment, where reaching the server code would fail on its missing token instead.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn bridge(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_metocast-server"));
    command.arg("mcp-stdio").env_clear().current_dir(dir);
    command
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("metocast-mcp-stdio-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn check_with_nothing_running_says_so_and_creates_nothing() {
    let dir = scratch("check");
    // A port nobody listens on.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}/mcp", closed.local_addr().unwrap());
    drop(closed);

    let output = bridge(&dir)
        .args(["--check", "--url", &address])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not running"), "{stderr}");
    assert!(output.stdout.is_empty(), "stdout carries protocol only");
    // The server would have made ./data here.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn closing_stdin_ends_it_quietly() {
    let dir = scratch("stdin");
    let output = bridge(&dir).stdin(Stdio::null()).output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn an_unknown_flag_is_a_usage_error() {
    let dir = scratch("usage");
    let output = bridge(&dir).arg("--nope").output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
    std::fs::remove_dir(&dir).unwrap();
}
