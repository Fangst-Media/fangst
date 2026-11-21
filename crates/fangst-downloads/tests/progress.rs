use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fangst_bench::{Config, Library, MockServer};
use fangst_downloads::{Change, Progress, Runner, Service, Status, Store};
use fangst_nntp::{Pool, ServerConfig};
use tokio::sync::broadcast;

const SEGMENT: usize = 5000;
const WAIT: Duration = Duration::from_secs(30);

fn library(len: usize) -> (Library, Vec<String>) {
    let file: Vec<u8> = (0..len).map(|i| i.to_le_bytes()[0]).collect();
    let mut library = Library::new();
    let ids = library.add_file("payload.bin", &file, SEGMENT, "alt.binaries.test");
    (library, ids)
}

/// One connection fetching one segment at a time, so the latency paces the job over seconds.
fn runner(store: Store, server: &MockServer, root: &std::path::Path) -> Runner {
    Runner {
        store,
        servers: vec![fangst_usenet::Server::primary(Arc::new(Pool::new(
            ServerConfig {
                host: "127.0.0.1".to_owned(),
                port: server.addr.port(),
                tls: None,
                credentials: None,
                max_connections: 1,
                connect_timeout: Duration::from_secs(5),
                command_timeout: Duration::from_secs(5),
            },
        )))],
        root: root.to_owned(),
        options: fangst_usenet::Options {
            connections: 1,
            pipeline_depth: 1,
            retries: 1,
        },
        post_processors: Vec::new(),
    }
}

fn slow(missing: HashSet<String>) -> Config {
    Config {
        latency: Duration::from_millis(50),
        missing,
        ..Config::default()
    }
}

/// Progress messages for `id` until the job leaves `downloading`, with when each arrived.
async fn progress_until_done(
    changes: &mut broadcast::Receiver<Change>,
    store: &Store,
    id: i64,
) -> Vec<(Instant, Progress)> {
    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let change = tokio::time::timeout_at(deadline, changes.recv())
            .await
            .expect("job finishes in time")
            .expect("change stream stays open");
        match change {
            Change::Progress(job, progress) if job == id => seen.push((Instant::now(), progress)),
            Change::Updated(job) if job == id => {
                let status = store.get(id).await.unwrap().status;
                if matches!(status, Status::Completed | Status::Failed) {
                    return seen;
                }
            }
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_job_reports_bytes_rate_and_time_left_about_once_a_second() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    // 60 segments at 50 ms each: about three seconds.
    let (lib, _) = library(60 * SEGMENT);
    let server = MockServer::start(lib.clone(), slow(HashSet::new()))
        .await
        .unwrap();
    let service = Service::new(runner(store.clone(), &server, dir.path()));
    let mut changes = service.subscribe();
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });

    let started = Instant::now();
    let id = service
        .add_nzb("progress", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let seen = progress_until_done(&mut changes, &store, id).await;
    let elapsed = started.elapsed();
    worker.abort();

    assert!(seen.len() >= 2, "only {} progress messages", seen.len());
    let allowed = usize::try_from(elapsed.as_secs()).unwrap() + 2;
    assert!(
        seen.len() <= allowed,
        "{} progress messages in {elapsed:?}",
        seen.len()
    );
    for pair in seen.windows(2) {
        let gap = pair[1].0.duration_since(pair[0].0);
        assert!(gap >= Duration::from_millis(500), "messages {gap:?} apart");
        assert!(
            pair[1].1.downloaded_bytes >= pair[0].1.downloaded_bytes,
            "downloaded bytes went back: {seen:?}"
        );
    }
    let total = u64::try_from(60 * SEGMENT).unwrap();
    // The first sample has nothing to compare with; every later one measures a rate.
    for (_, progress) in &seen[1..] {
        if progress.downloaded_bytes < total {
            assert!(progress.rate_bytes_per_sec > 0, "{progress:?}");
            assert!(progress.eta_seconds.is_some(), "{progress:?}");
        }
        assert!(progress.downloaded_bytes <= total);
    }

    let job = store.get(id).await.unwrap();
    assert_eq!(job.status, Status::Completed, "{:?}", job.error);
    assert_eq!(job.downloaded_bytes, job.total_bytes);
    assert_eq!(job.rate_bytes_per_sec, 0);
    assert_eq!(job.eta_seconds, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_api_view_of_a_running_job_is_live() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let (lib, _) = library(60 * SEGMENT);
    let server = MockServer::start(lib.clone(), slow(HashSet::new()))
        .await
        .unwrap();
    let service = Service::new(runner(store.clone(), &server, dir.path()));
    let mut changes = service.subscribe();
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });
    let id = service
        .add_nzb("live", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();

    // Two samples give a rate; the job listing must show the same live numbers.
    let mut measured = 0;
    while measured < 2 {
        if let Ok(Change::Progress(job, _)) = changes.recv().await
            && job == id
        {
            measured += 1;
        }
    }
    let job = store
        .list()
        .await
        .unwrap()
        .into_iter()
        .find(|j| j.id == id)
        .unwrap();
    worker.abort();
    assert_eq!(job.status, Status::Downloading);
    assert!(job.downloaded_bytes > 0 && job.downloaded_bytes < job.total_bytes);
    assert!(job.rate_bytes_per_sec > 0);
    assert!(job.eta_seconds.is_some());
}

#[tokio::test]
async fn a_failed_job_keeps_the_bytes_it_got() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let (lib, ids) = library(4 * SEGMENT);
    let server = MockServer::start(lib.clone(), slow(HashSet::from([ids[1].clone()])))
        .await
        .unwrap();
    let id = store
        .add_nzb("holes", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();

    runner(store.clone(), &server, dir.path())
        .run_until_idle()
        .await
        .unwrap();

    let job = store.get(id).await.unwrap();
    assert_eq!(job.status, Status::Failed);
    assert_eq!(
        job.downloaded_bytes,
        job.total_bytes - i64::try_from(SEGMENT).unwrap()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_shows_the_bytes_recorded_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("downloads.db");
    let store = Store::open(&db).await.unwrap();
    let (lib, ids) = library(200 * SEGMENT);
    let server = MockServer::start(lib.clone(), slow(HashSet::new()))
        .await
        .unwrap();
    let id = store
        .add_nzb("stopped", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();

    // The process stops mid-job: the runner is dropped without finishing.
    let first = runner(store.clone(), &server, dir.path());
    let task = tokio::spawn(async move { first.run_until_idle().await });
    let deadline = tokio::time::Instant::now() + WAIT;
    while store.done_segments(id).await.unwrap().len() < 10 {
        assert!(tokio::time::Instant::now() < deadline, "no progress");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    task.abort();
    let _ = task.await;

    let reopened = Store::open(&db).await.unwrap();
    let mut recorded = 0;
    while tokio::time::Instant::now() < deadline {
        recorded = reopened.get(id).await.unwrap().downloaded_bytes;
        if recorded > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let total = i64::try_from(ids.len() * SEGMENT).unwrap();
    assert!(
        recorded >= 10 * i64::try_from(SEGMENT).unwrap() && recorded < total,
        "recorded {recorded} of {total}"
    );
    reopened.requeue_interrupted().await.unwrap();
    let job = reopened.get(id).await.unwrap();
    assert_eq!(job.status, Status::Queued);
    assert_eq!(job.downloaded_bytes, recorded, "requeueing keeps the count");
    assert_eq!(job.rate_bytes_per_sec, 0);
    assert_eq!(job.eta_seconds, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn pausing_records_the_bytes_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let (lib, _) = library(200 * SEGMENT);
    let server = MockServer::start(lib.clone(), slow(HashSet::new()))
        .await
        .unwrap();
    let service = Service::new(runner(store.clone(), &server, dir.path()));
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });
    let id = service
        .add_nzb("pause me", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + WAIT;
    while store.done_segments(id).await.unwrap().len() < 5 {
        assert!(tokio::time::Instant::now() < deadline, "no progress");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let paused = service.pause(id).await.unwrap();
    worker.abort();
    assert_eq!(paused.status, Status::Paused);
    assert!(
        paused.downloaded_bytes >= 5 * i64::try_from(SEGMENT).unwrap(),
        "{paused:?}"
    );
    assert_eq!(paused.rate_bytes_per_sec, 0);
}
