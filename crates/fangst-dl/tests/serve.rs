//! Boots the real binary with nothing optional configured: no usenet server, every adapter off.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

struct Daemon {
    child: Child,
    addr: String,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(data_dir: &std::path::Path) -> Daemon {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
        .args([
            "serve",
            "--data-dir",
            data_dir.to_str().unwrap(),
            "--listen",
            "127.0.0.1:0",
        ])
        .env("FANGST_API_KEY", "a-long-enough-test-key")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first_line = String::new();
    BufReader::new(child.stdout.as_mut().unwrap())
        .read_line(&mut first_line)
        .unwrap();
    let addr = first_line
        .trim()
        .strip_prefix("listening on ")
        .unwrap_or_else(|| panic!("{first_line:?}"))
        .to_owned();
    Daemon { child, addr }
}

fn http(addr: &str, path: &str, key: Option<&str>) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).unwrap();
    let key_header = key
        .map(|k| format!("X-Api-Key: {k}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\n{key_header}Connection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, response)
}

#[test]
fn boots_with_nothing_optional_and_answers_health() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path());

    let (status, body) = http(&daemon.addr, "/api/v1/health", None);
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"status\":\"ok\""));
    assert_eq!(http(&daemon.addr, "/api/v1/jobs", None).0, 401);
    assert_eq!(
        http(&daemon.addr, "/api/v1/jobs", Some("a-long-enough-test-key")).0,
        200
    );
    assert!(dir.path().join("downloads.db").exists());
    assert_eq!(
        std::fs::read(dir.path().join("master.key")).unwrap().len(),
        32,
        "the master key is created on first start"
    );
}

// A placeholder, never a real credential.
const NEW_PASSWORD: &str = "placeholder-password";

fn login(addr: &str, password: &str) -> u16 {
    let body = format!(r#"{{"username":"admin","password":"{password}"}}"#);
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "POST /api/v1/auth/login HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response.split_whitespace().nth(1).unwrap().parse().unwrap()
}

fn reset_password(data_dir: &std::path::Path, user: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fangst-dl"));
    command.args([
        "reset-password",
        "--data-dir",
        data_dir.to_str().unwrap(),
        "--user",
        user,
    ]);
    command
}

#[test]
fn reset_password_from_the_environment_or_stdin_lets_the_user_log_in() {
    let dir = tempfile::tempdir().unwrap();
    drop(start(dir.path()));

    let output = reset_password(dir.path(), "admin")
        .args(["--password-env", "FANGST_TEST_NEW_PASSWORD"])
        .env("FANGST_TEST_NEW_PASSWORD", NEW_PASSWORD)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(NEW_PASSWORD));
    let daemon = start(dir.path());
    assert_eq!(login(&daemon.addr, NEW_PASSWORD), 200);
    drop(daemon);

    let mut child = reset_password(dir.path(), "ADMIN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"another-placeholder\r\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let daemon = start(dir.path());
    assert_eq!(login(&daemon.addr, "another-placeholder"), 200);
    assert_eq!(login(&daemon.addr, NEW_PASSWORD), 401);
}

#[test]
fn reset_password_refuses_unknown_users_wrong_folders_and_argv_passwords() {
    let dir = tempfile::tempdir().unwrap();
    let output = reset_password(dir.path(), "admin")
        .args(["--password-env", "FANGST_TEST_NEW_PASSWORD"])
        .env("FANGST_TEST_NEW_PASSWORD", NEW_PASSWORD)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--data-dir"));
    assert!(
        !dir.path().join("downloads.db").exists(),
        "a wrong folder is not turned into a new install"
    );

    drop(start(dir.path()));
    let output = reset_password(dir.path(), "nobody")
        .args(["--password-env", "FANGST_TEST_NEW_PASSWORD"])
        .env("FANGST_TEST_NEW_PASSWORD", NEW_PASSWORD)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no user named nobody"));

    let output = reset_password(dir.path(), "admin")
        .args(["--password", NEW_PASSWORD])
        .output()
        .unwrap();
    assert!(!output.status.success(), "there is no argument for it");
}

#[test]
fn refuses_a_short_api_key() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
        .args([
            "serve",
            "--data-dir",
            dir.path().to_str().unwrap(),
            "--listen",
            "127.0.0.1:0",
        ])
        .env("FANGST_API_KEY", "short")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("at least 16 characters"));
}

#[test]
fn committed_openapi_document_is_current() {
    let path = manifest_dir().join("../../openapi/dl.json");
    let output = Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
        .args(["openapi".as_ref(), "--check".as_ref(), path.as_os_str()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unknown_provider_stops_startup_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
        .args([
            "serve",
            "--data-dir",
            dir.path().to_str().unwrap(),
            "--listen",
            "127.0.0.1:0",
            "--enable",
            "notify-nope",
        ])
        .env("FANGST_API_KEY", "a-long-enough-test-key")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown provider `notify-nope`"));
}

/// The crate folder at run time. `env!("CARGO_MANIFEST_DIR")` is baked in at compile time, and a
/// build folder shared between git worktrees can hand back a test binary built in another (perhaps
/// deleted) worktree; cargo and nextest set the variable correctly for every run.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("run through cargo or nextest"),
    )
}
