//! Boots the real binary with nothing optional configured: no usenet server, every adapter off,
//! and the torrent engine on with nothing that reaches past this machine (DHT, local discovery and
//! `UPnP` off), or off altogether.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

struct Daemon {
    child: Child,
    addr: String,
    /// What the daemon printed up to its provider listing.
    banner: Vec<String>,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Keeps the torrent engine on this machine: a free port, no DHT, no multicast, no router.
const LOCAL_TORRENTS: &[&str] = &[
    "--torrent-port",
    "0",
    "--no-dht",
    "--no-local-discovery",
    "--no-upnp",
];

fn start(data_dir: &std::path::Path) -> Daemon {
    start_with(data_dir, LOCAL_TORRENTS)
}

fn start_with(data_dir: &std::path::Path, extra: &[&str]) -> Daemon {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
        .args([
            "serve",
            "--data-dir",
            data_dir.to_str().unwrap(),
            "--listen",
            "127.0.0.1:0",
        ])
        .args(extra)
        .env("FANGST_API_KEY", KEY)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut banner = Vec::new();
    loop {
        let mut line = String::new();
        if stdout.read_line(&mut line).unwrap() == 0 {
            let mut stderr = String::new();
            child
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut stderr)
                .unwrap();
            panic!("the daemon stopped after {banner:?}: {stderr}");
        }
        let line = line.trim().to_owned();
        let last = line.starts_with("providers:");
        banner.push(line);
        if last {
            break;
        }
    }
    let addr = banner[0]
        .strip_prefix("listening on ")
        .unwrap_or_else(|| panic!("{banner:?}"))
        .to_owned();
    Daemon {
        child,
        addr,
        banner,
    }
}

fn http(addr: &str, path: &str, key: Option<&str>) -> (u16, String) {
    request(addr, "GET", path, key, "")
}

fn request(addr: &str, method: &str, path: &str, key: Option<&str>, json: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).unwrap();
    let key_header = key
        .map(|k| format!("X-Api-Key: {k}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{key_header}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
        json.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, response)
}

const KEY: &str = "a-long-enough-test-key";

/// Adds a magnet link no peer can be found for, waits until the job has failed or `wait` passes,
/// and returns the job as the API shows it.
fn add_unresolvable_magnet(addr: &str, wait: std::time::Duration) -> String {
    let link = format!("magnet:?xt=urn:btih:{}&dn=Nobody", "ab".repeat(20));
    let (status, body) = request(
        addr,
        "POST",
        "/api/v1/jobs",
        Some(KEY),
        &format!(r#"{{"magnet":"{link}"}}"#),
    );
    assert_eq!(status, 201, "{body}");
    let id: String = body
        .split("\"id\":")
        .nth(1)
        .unwrap()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let deadline = std::time::Instant::now() + wait;
    loop {
        let (_, body) = http(addr, &format!("/api/v1/jobs/{id}"), Some(KEY));
        if body.contains("\"status\":\"failed\"") || std::time::Instant::now() > deadline {
            return body;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn torrents_run_without_a_usenet_server() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path());
    assert!(
        daemon
            .banner
            .iter()
            .any(|l| l.starts_with("torrents: listening on ")),
        "{:?}",
        daemon.banner
    );
    // With every way to find peers off, the engine gives up on the link at once, which shows the
    // torrent lane picked the job up although no usenet server is configured.
    let job = add_unresolvable_magnet(&daemon.addr, std::time::Duration::from_secs(10));
    assert!(job.contains("\"status\":\"failed\""), "{job}");
    assert!(job.contains("resolve peers"), "{job}");
}

#[test]
fn boots_with_torrents_off_and_leaves_torrent_jobs_queued() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start_with(dir.path(), &["--no-torrent"]);
    assert!(
        daemon.banner.iter().any(|l| l.starts_with("torrents: off")),
        "{:?}",
        daemon.banner
    );
    assert_eq!(http(&daemon.addr, "/api/v1/health", None).0, 200);
    // Long enough for a running torrent lane to have failed the link, as in the test above.
    let job = add_unresolvable_magnet(&daemon.addr, std::time::Duration::from_secs(1));
    assert!(job.contains("\"status\":\"queued\""), "{job}");
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
        .env("FANGST_API_KEY", KEY)
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
