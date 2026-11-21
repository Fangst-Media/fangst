//! The SABnzbd adapter in the real binary: off unless enabled, `apikey` checked against the user
//! keys, and `addurl` fetching from a local HTTP server.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};

// A placeholder, never a real credential.
const KEY: &str = "sabnzbd-test-key-placeholder";

const NZB: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<nzb xmlns="http://www.newzbin.com/DTD/2003/nzb">
<file poster="tester" date="1700000000" subject="&quot;a.bin&quot; yEnc (1/1)">
<groups><group>alt.binaries.test</group></groups>
<segments><segment bytes="1000" number="1">part1@test</segment></segments>
</file>
</nzb>"#;

struct Daemon {
    child: Child,
    addr: String,
    _dir: tempfile::TempDir,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(enable: &[&str]) -> Daemon {
    let dir = tempfile::tempdir().unwrap();
    let mut args = vec![
        "serve".to_owned(),
        "--data-dir".to_owned(),
        dir.path().to_str().unwrap().to_owned(),
        "--listen".to_owned(),
        "127.0.0.1:0".to_owned(),
        // The torrent engine isn't under test; keep it off the network.
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
    Daemon {
        child,
        addr,
        _dir: dir,
    }
}

/// GET `path`; returns the status and the body.
fn get(addr: &str, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_owned())
        .unwrap_or_default();
    (status, body)
}

/// Serves the NZB at any path, for as many requests as come, until the test ends.
fn nzb_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-nzb\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{NZB}",
                NZB.len()
            );
        }
    });
    format!("http://{addr}")
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[test]
fn off_unless_enabled() {
    let daemon = start(&[]);
    assert_eq!(get(&daemon.addr, "/api?mode=version&output=json").0, 404);
    assert_eq!(
        get(&daemon.addr, &format!("/api?mode=queue&apikey={KEY}")).0,
        404
    );
}

#[test]
fn speaks_sabnzbd_with_the_user_api_key() {
    let daemon = start(&["adapter-sabnzbd"]);
    let (status, body) = get(&daemon.addr, "/api?mode=version&output=json");
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"version\""), "{body}");

    assert_eq!(
        get(&daemon.addr, "/api?mode=queue&output=json"),
        (403, "API Key Required".to_owned())
    );
    assert_eq!(
        get(&daemon.addr, "/api?mode=queue&apikey=wrong-key&output=json"),
        (403, "API Key Incorrect".to_owned())
    );
    let (status, body) = get(
        &daemon.addr,
        &format!("/api?mode=queue&apikey={KEY}&output=json"),
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"noofslots_total\":0"), "{body}");

    // The native API is untouched by the adapter at /api.
    assert_eq!(get(&daemon.addr, "/api/v1/health").0, 200);
    assert_eq!(get(&daemon.addr, "/api/v1/jobs").0, 401);
}

#[test]
fn addurl_fetches_the_nzb_and_queues_it() {
    let daemon = start(&["adapter-sabnzbd"]);
    let url = format!("{}/getnzb/Some.Show.S01E01.nzb", nzb_server());
    let (status, body) = get(
        &daemon.addr,
        &format!(
            "/api?mode=addurl&name={}&cat=tv&apikey={KEY}&output=json",
            encode(&url)
        ),
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains(r#""status":true"#), "{body}");
    assert!(body.contains("SABnzbd_nzo_1"), "{body}");

    let (_, queue) = get(
        &daemon.addr,
        &format!("/api?mode=queue&category=tv&apikey={KEY}&output=json"),
    );
    assert!(
        queue.contains(r#""filename":"Some.Show.S01E01""#),
        "{queue}"
    );

    let (status, body) = get(
        &daemon.addr,
        &format!(
            "/api?mode=addurl&name={}&apikey={KEY}&output=json",
            encode("file:///etc/passwd")
        ),
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains(r#""status":false"#), "{body}");
    assert!(body.contains("only http and https"), "{body}");
}
