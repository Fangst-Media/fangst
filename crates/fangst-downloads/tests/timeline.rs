use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_downloads::{Runner, Status, Store};
use fangst_nntp::{Pool, ServerConfig};

fn runner(store: Store, server: &MockServer, root: &std::path::Path) -> Runner {
    Runner {
        store,
        servers: vec![fangst_usenet::Server::primary(Arc::new(Pool::new(
            ServerConfig {
                host: "127.0.0.1".to_owned(),
                port: server.addr.port(),
                tls: None,
                credentials: None,
                max_connections: 2,
                connect_timeout: Duration::from_secs(5),
                command_timeout: Duration::from_secs(5),
            },
        )))],
        root: root.to_owned(),
        options: fangst_usenet::Options {
            connections: 2,
            pipeline_depth: 2,
            retries: 1,
        },
    }
}

fn kinds(events: &[fangst_downloads::Event]) -> Vec<&str> {
    events.iter().map(|e| e.kind.as_str()).collect()
}

#[tokio::test]
async fn completed_job_has_a_full_timeline() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::new();
    library.add_file("a.bin", &vec![7u8; 20_000], 5000, "g");
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let id = store
        .add_nzb("done", None, 0, library.nzb().as_bytes())
        .await
        .unwrap();
    let server = MockServer::start(library, Config::default()).await.unwrap();

    runner(store.clone(), &server, dir.path())
        .run_until_idle()
        .await
        .unwrap();

    let events = store.timeline(id).await.unwrap();
    assert_eq!(
        kinds(&events),
        ["added", "status", "file", "server", "status"]
    );
    assert_eq!(events[0].message, "added with 1 files, 20000 bytes");
    assert_eq!(events[1].message, "downloading");
    assert_eq!(events[2].message, "a.bin: 4 of 4 segments stored");
    assert_eq!(
        events[3].message,
        "server 127.0.0.1: served 4 segments, missing 0"
    );
    assert_eq!(events[4].message, "completed");
}

#[tokio::test]
async fn failed_job_timeline_names_the_cause() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::new();
    let ids = library.add_file("b.bin", &vec![1u8; 20_000], 5000, "g");
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let id = store
        .add_nzb("broken", None, 0, library.nzb().as_bytes())
        .await
        .unwrap();
    let config = Config {
        missing: HashSet::from([ids[1].clone(), ids[3].clone()]),
        ..Config::default()
    };
    let server = MockServer::start(library, config).await.unwrap();

    runner(store.clone(), &server, dir.path())
        .run_until_idle()
        .await
        .unwrap();

    assert_eq!(store.get(id).await.unwrap().status, Status::Failed);
    let events = store.timeline(id).await.unwrap();
    let file = events.iter().find(|e| e.kind == "file").unwrap();
    assert_eq!(
        file.message,
        "b.bin: 2 of 4 segments stored, 2 missing on the server (430)"
    );
    assert_eq!(
        events.last().unwrap().message,
        "failed: 2 segments missing on the server, 0 could not be downloaded"
    );
}

#[tokio::test]
async fn interrupted_job_records_requeue_and_resume() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::new();
    library.add_file("c.bin", &vec![3u8; 20_000], 5000, "g");
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let id = store
        .add_nzb("again", None, 0, library.nzb().as_bytes())
        .await
        .unwrap();
    // A process that stopped mid-download leaves the job in `downloading`.
    store
        .set_status(id, Status::Downloading, None)
        .await
        .unwrap();
    let server = MockServer::start(library, Config::default()).await.unwrap();

    runner(store.clone(), &server, dir.path())
        .run_until_idle()
        .await
        .unwrap();

    let events = store.timeline(id).await.unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.message == "queued: the previous run stopped before this job finished")
    );
    assert_eq!(store.get(id).await.unwrap().status, Status::Completed);
}

#[tokio::test]
async fn timeline_names_which_server_served_and_missed() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::new();
    let ids = library.add_file("d.bin", &vec![5u8; 20_000], 5000, "g");
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let id = store
        .add_nzb("backfilled", None, 0, library.nzb().as_bytes())
        .await
        .unwrap();
    let primary = MockServer::start(
        library.clone(),
        Config {
            missing: HashSet::from([ids[2].clone()]),
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let backfill = MockServer::start(library, Config::default()).await.unwrap();
    let mut job_runner = runner(store.clone(), &primary, dir.path());
    let backfill_server = runner(store.clone(), &backfill, dir.path())
        .servers
        .remove(0);
    job_runner.servers.push(fangst_usenet::Server {
        name: "block".to_owned(),
        backfill: true,
        ..backfill_server
    });
    job_runner.servers[0].name = "main".to_owned();

    job_runner.run_until_idle().await.unwrap();

    assert_eq!(store.get(id).await.unwrap().status, Status::Completed);
    let servers: Vec<String> = store
        .timeline(id)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "server")
        .map(|e| e.message)
        .collect();
    assert_eq!(
        servers,
        [
            "server main: served 3 segments, missing 1",
            "backfill server block: served 1 segments, missing 0"
        ]
    );
}
