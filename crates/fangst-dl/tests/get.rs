use std::collections::HashSet;
use std::process::Command;

use fangst_bench::{Config, Library, MockServer};

fn data(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| {
            u32::try_from(i)
                .unwrap()
                .wrapping_mul(2_654_435_761)
                .to_le_bytes()[3]
        })
        .collect()
}

async fn run_get(
    server: &MockServer,
    nzb: &std::path::Path,
    out: &std::path::Path,
) -> std::process::Output {
    let args = vec![
        "get".to_owned(),
        nzb.display().to_string(),
        "--out".to_owned(),
        out.display().to_string(),
        "--host".to_owned(),
        "127.0.0.1".to_owned(),
        "--port".to_owned(),
        server.addr.port().to_string(),
        "--no-tls".to_owned(),
        "--connections".to_owned(),
        "2".to_owned(),
    ];
    tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
            .args(args)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn get_downloads_an_nzb_and_reports_each_file() {
    let file = data(80_000);
    let mut library = Library::new();
    library.add_file("movie.mkv", &file, 9000, "alt.binaries.test");
    let dir = tempfile::tempdir().unwrap();
    let nzb_path = dir.path().join("job.nzb");
    std::fs::write(&nzb_path, library.nzb()).unwrap();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let out = dir.path().join("out");

    let output = run_get(&server, &nzb_path, &out).await;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("done  movie.mkv  (80000 bytes)"),
        "{stdout}"
    );
    assert_eq!(std::fs::read(out.join("movie.mkv")).unwrap(), file);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_exits_with_2_and_lists_missing_segments() {
    let mut library = Library::new();
    let ids = library.add_file("holes.bin", &data(30_000), 10_000, "g");
    let dir = tempfile::tempdir().unwrap();
    let nzb_path = dir.path().join("job.nzb");
    std::fs::write(&nzb_path, library.nzb()).unwrap();
    let config = Config {
        missing: HashSet::from([ids[1].clone()]),
        ..Config::default()
    };
    let server = MockServer::start(library, config).await.unwrap();

    let output = run_get(&server, &nzb_path, &dir.path().join("out")).await;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2), "{stdout}");
    assert!(
        stdout.contains("missing  holes.bin  segments [2]"),
        "{stdout}"
    );
}

#[test]
fn bad_nzb_is_a_readable_error() {
    let dir = tempfile::tempdir().unwrap();
    let nzb_path = dir.path().join("broken.nzb");
    std::fs::write(&nzb_path, "<html/>").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fangst-dl"))
        .args([
            "get",
            nzb_path.to_str().unwrap(),
            "--out",
            "x",
            "--host",
            "localhost",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("no <nzb> root element"), "{stderr}");
}
