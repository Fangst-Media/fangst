//! The qBittorrent adapter in the real binary: off unless enabled, signing in with a user's
//! password or API key, the `SID` cookie, and a torrent added, listed and deleted. The torrent
//! engine is off, so added torrents stay queued and nothing reaches the network.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};

// Placeholders only; never real credentials.
const KEY: &str = "qbittorrent-test-key-placeholder";
const PASSWORD: &str = "placeholder-password";
const HASH: &str = "cab507494d02ebb1178b38f2e9d7be299c86b862";

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

fn start(data_dir: &std::path::Path, enable: &[&str]) -> Daemon {
    let mut args = vec![
        "serve".to_owned(),
        "--data-dir".to_owned(),
        data_dir.to_str().unwrap().to_owned(),
        "--listen".to_owned(),
        "127.0.0.1:0".to_owned(),
        "--no-torrent".to_owned(),
    ];
    for id in enable {
        args.push("--enable".to_owned());
        args.push((*id).to_owned());
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
        .args(&args)
        .env("FANGST_API_KEY", KEY)
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

struct Reply {
    status: u16,
    headers: String,
    body: String,
}

fn send(addr: &str, method: &str, path: &str, cookie: Option<&str>, form: &str) -> Reply {
    let mut stream = TcpStream::connect(addr).unwrap();
    let cookie = cookie
        .map(|c| format!("Cookie: SID={c}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{cookie}Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{form}",
        form.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    Reply {
        status,
        headers: headers.to_ascii_lowercase(),
        body: body.to_owned(),
    }
}

fn login(addr: &str, username: &str, password: &str) -> Reply {
    send(
        addr,
        "POST",
        "/api/v2/auth/login",
        None,
        &format!("username={username}&password={password}"),
    )
}

fn sid(reply: &Reply) -> String {
    reply
        .headers
        .lines()
        .find_map(|line| line.strip_prefix("set-cookie: sid="))
        .and_then(|rest| rest.split(';').next())
        .unwrap_or_else(|| panic!("no SID cookie in {}", reply.headers))
        .to_owned()
}

#[test]
fn off_unless_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = start(dir.path(), &[]);
    assert_eq!(login(&daemon.addr, "admin", KEY).status, 404);
    assert_eq!(
        send(&daemon.addr, "GET", "/api/v2/app/version", None, "").status,
        404
    );
}

#[test]
fn signs_in_with_an_api_key_and_manages_torrents() {
    let dir = tempfile::tempdir().unwrap();
    // Next to the SABnzbd adapter at /api, which must keep working.
    let daemon = start(dir.path(), &["adapter-qbittorrent", "adapter-sabnzbd"]);
    let addr = daemon.addr.as_str();

    let wrong = login(addr, "admin", "wrong-password");
    assert_eq!((wrong.status, wrong.body.as_str()), (200, "Fails."));
    assert!(!wrong.headers.contains("set-cookie"));
    assert_eq!(
        send(addr, "GET", "/api/v2/app/webapiVersion", None, "").status,
        403
    );
    assert_eq!(
        send(addr, "GET", "/api/v2/app/webapiVersion", Some("forged"), "").status,
        403
    );

    let signed_in = login(addr, "sonarr", KEY);
    assert_eq!((signed_in.status, signed_in.body.as_str()), (200, "Ok."));
    let cookie = sid(&signed_in);
    let session = Some(cookie.as_str());
    let version = send(addr, "GET", "/api/v2/app/webapiVersion", session, "");
    assert_eq!((version.status, version.body.as_str()), (200, "2.11.2"));

    let created = send(
        addr,
        "POST",
        "/api/v2/torrents/createCategory",
        session,
        "category=tv-sonarr",
    );
    assert_eq!(created.status, 200, "{}", created.body);
    let added = send(
        addr,
        "POST",
        "/api/v2/torrents/add",
        session,
        &format!(
            "urls=magnet%3A%3Fxt%3Durn%3Abtih%3A{HASH}%26dn%3DSome.Show&category=tv-sonarr&stopped=False"
        ),
    );
    assert_eq!((added.status, added.body.as_str()), (200, "Ok."));

    let listed = send(
        addr,
        "GET",
        "/api/v2/torrents/info?category=tv-sonarr",
        session,
        "",
    );
    assert_eq!(listed.status, 200, "{}", listed.body);
    assert!(
        listed.body.contains(&format!(r#""hash":"{HASH}""#)),
        "{}",
        listed.body
    );
    assert!(
        listed.body.contains(r#""state":"queuedDL""#),
        "{}",
        listed.body
    );
    assert!(
        listed.body.contains(r#""name":"Some.Show""#),
        "{}",
        listed.body
    );

    let deleted = send(
        addr,
        "POST",
        "/api/v2/torrents/delete",
        session,
        &format!("hashes={HASH}&deleteFiles=false"),
    );
    assert_eq!(deleted.status, 200, "{}", deleted.body);
    let listed = send(addr, "GET", "/api/v2/torrents/info", session, "");
    assert_eq!(listed.body, "[]");

    assert_eq!(
        send(addr, "GET", "/api?mode=version&output=json", None, "").status,
        200,
        "SABnzbd still answers at /api"
    );
    assert_eq!(send(addr, "GET", "/api/v1/health", None, "").status, 200);
}

#[test]
fn signs_in_with_a_users_password() {
    let dir = tempfile::tempdir().unwrap();
    drop(start(dir.path(), &[]));
    let output = Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
        .args([
            "reset-password",
            "--data-dir",
            dir.path().to_str().unwrap(),
            "--password-env",
            "FANGST_TEST_PASSWORD",
        ])
        .env("FANGST_TEST_PASSWORD", PASSWORD)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let daemon = start(dir.path(), &["adapter-qbittorrent"]);
    assert_eq!(login(&daemon.addr, "sonarr", PASSWORD).body, "Fails.");
    let signed_in = login(&daemon.addr, "admin", PASSWORD);
    assert_eq!(signed_in.body, "Ok.");
    let cookie = sid(&signed_in);
    let info = send(
        &daemon.addr,
        "GET",
        "/api/v2/torrents/info",
        Some(&cookie),
        "",
    );
    assert_eq!((info.status, info.body.as_str()), (200, "[]"));
}
