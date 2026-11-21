use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_downloads::{Error, Runner, Status, Store};
use fangst_nntp::{Pool, ServerConfig};

fn data(len: usize, seed: u32) -> Vec<u8> {
    (0..len)
        .map(|i| u32::try_from(i).unwrap().wrapping_mul(seed).to_le_bytes()[2])
        .collect()
}

fn library(len: usize, segment: usize) -> (Library, Vec<u8>, Vec<String>) {
    let file = data(len, 2_654_435_761);
    let mut library = Library::new();
    let ids = library.add_file("payload.bin", &file, segment, "alt.binaries.test");
    (library, file, ids)
}

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

#[tokio::test]
async fn queued_jobs_come_out_by_priority_then_age() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let (lib, _, _) = library(1000, 1000);
    let nzb = lib.nzb();
    let low = store.add_nzb("low", None, 0, nzb.as_bytes()).await.unwrap();
    let high = store
        .add_nzb("high", Some("tv"), 10, nzb.as_bytes())
        .await
        .unwrap();
    let low_later = store
        .add_nzb("low later", None, 0, nzb.as_bytes())
        .await
        .unwrap();

    assert_eq!(store.next_queued().await.unwrap().unwrap().id, high);
    store
        .set_status(high, Status::Completed, None)
        .await
        .unwrap();
    assert_eq!(store.next_queued().await.unwrap().unwrap().id, low);

    let job = store.get(high).await.unwrap();
    assert_eq!(job.category.as_deref(), Some("tv"));
    assert_eq!(job.total_bytes, 1000);
    assert!(job.finished_at.is_some());
    let ids: Vec<i64> = store.list().await.unwrap().iter().map(|j| j.id).collect();
    assert_eq!(ids, [low_later, high, low]);
}

#[tokio::test]
async fn invalid_nzb_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let err = store.add_nzb("bad", None, 0, b"<html/>").await.unwrap_err();
    assert!(matches!(err, Error::InvalidNzb(_)));
    assert!(store.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn runner_completes_a_job() {
    let dir = tempfile::tempdir().unwrap();
    let (lib, file, _) = library(60_000, 6000);
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let id = store
        .add_nzb("Some Release", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let server = MockServer::start(lib, Config::default()).await.unwrap();

    runner(store.clone(), &server, dir.path())
        .run_until_idle()
        .await
        .unwrap();

    let job = store.get(id).await.unwrap();
    assert_eq!(job.status, Status::Completed, "{:?}", job.error);
    let out = job.output_path.unwrap();
    assert_eq!(out, dir.path().join(format!("{id}-Some Release")));
    assert_eq!(std::fs::read(out.join("payload.bin")).unwrap(), file);
}

#[tokio::test]
async fn missing_segments_fail_the_job_with_a_reason() {
    let dir = tempfile::tempdir().unwrap();
    let (lib, _, ids) = library(30_000, 10_000);
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let id = store
        .add_nzb("holes", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let config = Config {
        missing: HashSet::from([ids[0].clone()]),
        ..Config::default()
    };
    let server = MockServer::start(lib, config).await.unwrap();

    runner(store.clone(), &server, dir.path())
        .run_until_idle()
        .await
        .unwrap();

    let job = store.get(id).await.unwrap();
    assert_eq!(job.status, Status::Failed);
    assert_eq!(
        job.error.as_deref(),
        Some("1 segments missing on the server, 0 could not be downloaded")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_resumes_without_refetching_finished_segments() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("downloads.db");
    let (lib, file, ids) = library(200_000, 5000);
    let total = ids.len() as u64;
    let store = Store::open(&db).await.unwrap();
    let id = store
        .add_nzb("resume me", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let server = MockServer::start(
        lib,
        Config {
            latency: Duration::from_millis(15),
            ..Config::default()
        },
    )
    .await
    .unwrap();

    let first = runner(store.clone(), &server, dir.path());
    let task = tokio::spawn(async move { first.run_until_idle().await });
    let mut recorded = 0;
    for _ in 0..500 {
        recorded = store.done_segments(id).await.unwrap().len();
        if recorded >= 10 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    task.abort();
    let _ = task.await;
    // Requests already pipelined keep being answered (and counted) by the server for a moment,
    // and the recorder drains its queue; wait until both settle before taking the snapshot.
    let mut last = server.stats().bodies;
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let now = server.stats().bodies;
        if now == last {
            break;
        }
        last = now;
    }
    assert!(
        recorded >= 10 && (recorded as u64) < total,
        "recorded {recorded} of {total}"
    );
    assert_eq!(store.get(id).await.unwrap().status, Status::Downloading);
    let fetched_before = server.stats().bodies;
    let recorded_before = store.done_segments(id).await.unwrap().len() as u64;

    let reopened = Store::open(&db).await.unwrap();
    runner(reopened.clone(), &server, dir.path())
        .run_until_idle()
        .await
        .unwrap();

    let job = reopened.get(id).await.unwrap();
    assert_eq!(job.status, Status::Completed, "{:?}", job.error);
    let resumed = reopened
        .timeline(id)
        .await
        .unwrap()
        .into_iter()
        .find(|e| e.kind == "resumed")
        .unwrap();
    assert_eq!(
        resumed.message,
        format!("resuming: {recorded_before} of {total} segments already on disk")
    );
    assert_eq!(
        std::fs::read(job.output_path.unwrap().join("payload.bin")).unwrap(),
        file
    );
    let fetched_after = server.stats().bodies - fetched_before;
    assert_eq!(
        fetched_after,
        total - recorded_before,
        "only segments not recorded are fetched again"
    );
}

#[tokio::test]
async fn existing_database_is_backed_up_before_migrating() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("downloads.db");
    {
        use sqlx::Connection;
        let url = format!("sqlite://{}?mode=rwc", db.display());
        let mut conn = sqlx::SqliteConnection::connect(&url).await.unwrap();
        sqlx::query("CREATE TABLE keep_me (x INTEGER)")
            .execute(&mut conn)
            .await
            .unwrap();
        conn.close().await.unwrap();
    }

    Store::open(&db).await.unwrap();

    let backup = dir.path().join(newest_migration_backup());
    assert!(backup.exists(), "expected {}", backup.display());
    Store::open(&db).await.unwrap();
    assert!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".bak")
            })
            .count()
            == 1,
        "an up-to-date database is not backed up again"
    );
}

#[tokio::test]
async fn fresh_database_needs_no_backup() {
    let dir = tempfile::tempdir().unwrap();
    Store::open(&dir.path().join("downloads.db")).await.unwrap();
    assert!(!dir.path().join(newest_migration_backup()).exists());
}

/// The backup is named after the newest pending migration, which changes with every new one.
fn newest_migration_backup() -> String {
    let dir = manifest_dir().join("migrations");
    let newest = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            name.split_once('_')?.0.parse::<i64>().ok()
        })
        .max()
        .unwrap();
    format!("downloads.db.before-{newest}.bak")
}

/// The crate folder at run time. `env!("CARGO_MANIFEST_DIR")` is baked in at compile time, and a
/// build folder shared between git worktrees can hand back a test binary built in another (perhaps
/// deleted) worktree; cargo and nextest set the variable correctly for every run.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("run through cargo or nextest"),
    )
}
