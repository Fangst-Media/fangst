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
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../openapi/dl.json");
    let output = Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
        .args(["openapi", "--check", path])
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
