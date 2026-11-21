use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_nntp::{Pool, ServerConfig};
use fangst_usenet::{Options, Progress, download};

fn data(len: usize, seed: u32) -> Vec<u8> {
    (0..len)
        .map(|i| u32::try_from(i).unwrap().wrapping_mul(seed).to_le_bytes()[1])
        .collect()
}

fn pool(server: &MockServer, connections: usize) -> Arc<Pool> {
    Arc::new(Pool::new(ServerConfig {
        host: "127.0.0.1".to_owned(),
        port: server.addr.port(),
        tls: None,
        credentials: None,
        max_connections: connections,
        connect_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(5),
    }))
}

const OPTIONS: Options = Options {
    connections: 4,
    pipeline_depth: 8,
    retries: 2,
};

fn quiet() -> fangst_usenet::ProgressFn {
    Arc::new(|_: &Progress| {})
}

#[tokio::test]
async fn downloads_every_file_byte_for_byte() {
    let a = data(123_457, 7);
    let b = data(40_000, 13);
    let mut library = Library::new();
    library.add_file("first.bin", &a, 7000, "alt.binaries.test");
    library.add_file("second.bin", &b, 7000, "alt.binaries.test");
    let nzb = fangst_nzb::parse(library.nzb().as_bytes(), &fangst_nzb::Limits::default()).unwrap();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let out = tempfile::tempdir().unwrap();

    let report = download(&nzb, out.path(), pool(&server, 4), OPTIONS, quiet())
        .await
        .unwrap();

    assert!(report.is_complete(), "{report:?}");
    assert_eq!(std::fs::read(out.path().join("first.bin")).unwrap(), a);
    assert_eq!(std::fs::read(out.path().join("second.bin")).unwrap(), b);
}

#[tokio::test]
async fn missing_articles_are_reported_not_skipped() {
    let file = data(50_000, 3);
    let mut library = Library::new();
    let ids = library.add_file("holes.bin", &file, 5000, "g");
    let nzb = fangst_nzb::parse(library.nzb().as_bytes(), &fangst_nzb::Limits::default()).unwrap();
    let config = Config {
        missing: HashSet::from([ids[2].clone(), ids[7].clone()]),
        ..Config::default()
    };
    let server = MockServer::start(library, config).await.unwrap();
    let out = tempfile::tempdir().unwrap();

    let report = download(&nzb, out.path(), pool(&server, 2), OPTIONS, quiet())
        .await
        .unwrap();

    let file_report = &report.files[0];
    assert!(!report.is_complete());
    assert_eq!(file_report.missing_segments, [3, 8]);
    assert_eq!(file_report.bytes_written, 40_000);
    assert_eq!(file_report.size, 50_000);
}

#[tokio::test]
async fn progress_reaches_the_end_of_every_file() {
    let mut library = Library::new();
    library.add_file("a.bin", &data(30_000, 5), 3000, "g");
    library.add_file("b.bin", &data(9_000, 9), 3000, "g");
    let nzb = fangst_nzb::parse(library.nzb().as_bytes(), &fangst_nzb::Limits::default()).unwrap();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let out = tempfile::tempdir().unwrap();
    let finished = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&finished);
    let progress: fangst_usenet::ProgressFn = Arc::new(move |p: &Progress| {
        if p.file_finished() {
            seen.lock().unwrap().push(p.file_name.clone());
        }
    });

    download(&nzb, out.path(), pool(&server, 3), OPTIONS, progress)
        .await
        .unwrap();

    let mut names = finished.lock().unwrap().clone();
    names.sort();
    assert_eq!(names, ["a.bin", "b.bin"]);
}

#[tokio::test]
async fn hostile_file_names_stay_inside_the_folder() {
    let mut library = Library::new();
    library.add_file("../../escape.bin", &data(1000, 3), 1000, "g");
    let nzb = fangst_nzb::parse(library.nzb().as_bytes(), &fangst_nzb::Limits::default()).unwrap();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("job");

    let report = download(&nzb, &out, pool(&server, 1), OPTIONS, quiet())
        .await
        .unwrap();

    assert_eq!(report.files[0].path, out.join("escape.bin"));
    assert!(out.join("escape.bin").exists());
    assert!(!root.path().join("escape.bin").exists());
}

#[tokio::test]
async fn duplicate_names_get_a_suffix() {
    let mut library = Library::new();
    library.add_file("same.bin", &data(1000, 3), 1000, "g");
    library.add_file("same.bin", &data(1000, 5), 1000, "g");
    let nzb = fangst_nzb::parse(library.nzb().as_bytes(), &fangst_nzb::Limits::default()).unwrap();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let out = tempfile::tempdir().unwrap();

    let report = download(&nzb, out.path(), pool(&server, 2), OPTIONS, quiet())
        .await
        .unwrap();

    let names: Vec<&str> = report.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["same.bin", "same.bin.1"]);
    assert_eq!(
        std::fs::read(out.path().join("same.bin.1")).unwrap(),
        data(1000, 5)
    );
}

#[tokio::test]
async fn unreachable_server_fails_segments_after_retries() {
    let mut library = Library::new();
    library.add_file("x.bin", &data(3000, 3), 1000, "g");
    let nzb = fangst_nzb::parse(library.nzb().as_bytes(), &fangst_nzb::Limits::default()).unwrap();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let dead = pool(&server, 1);
    drop(server);
    let out = tempfile::tempdir().unwrap();

    let no_retries = Options {
        retries: 0,
        ..OPTIONS
    };
    let report = download(&nzb, out.path(), dead, no_retries, quiet())
        .await
        .unwrap();

    assert_eq!(report.files[0].failed_segments.len(), 3);
    assert_eq!(report.files[0].bytes_written, 0);
}
