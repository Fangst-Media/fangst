//! Settings that change while the daemon runs: usenet servers, notifiers and category folders.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_downloads::{Notifications, Runner, Service, Status, Store};
use fangst_nntp::{Pool, ServerConfig};
use fangst_providers::{
    BoxFuture, NotificationEvent, Notifier, ProviderError, ProviderInfo, ProviderKind,
};

const WAIT: Duration = Duration::from_secs(20);

fn library() -> Library {
    let file: Vec<u8> = (0..20_000u32).map(|i| i.to_le_bytes()[0]).collect();
    let mut library = Library::new();
    library.add_file("payload.bin", &file, 5000, "alt.binaries.test");
    library
}

fn server(mock: &MockServer) -> fangst_usenet::Server {
    fangst_usenet::Server::primary(Arc::new(Pool::new(ServerConfig {
        host: "127.0.0.1".to_owned(),
        port: mock.addr.port(),
        tls: None,
        credentials: None,
        max_connections: 2,
        connect_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(5),
    })))
}

fn options() -> fangst_usenet::Options {
    fangst_usenet::Options {
        connections: 2,
        pipeline_depth: 2,
        retries: 1,
    }
}

fn runner(store: Store, root: PathBuf, servers: Vec<fangst_usenet::Server>) -> Runner {
    Runner {
        store,
        servers,
        root,
        options: options(),
        post_processors: Vec::new(),
    }
}

async fn wait_for_status(store: &Store, id: i64, status: Status) -> Status {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let job = store.get(id).await.unwrap();
        if job.status == status || tokio::time::Instant::now() >= deadline {
            return job.status;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_server_added_while_running_starts_the_waiting_usenet_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let lib = library();
    let mock = MockServer::start(lib.clone(), Config::default())
        .await
        .unwrap();
    let service =
        Service::new(runner(store.clone(), dir.path().join("dl"), Vec::new())).without_usenet();
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });

    let id = service
        .add_nzb("Waits", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    assert_eq!(store.get(id).await.unwrap().status, Status::Queued);

    service.set_usenet(vec![server(&mock)], options());
    let status = wait_for_status(&store, id, Status::Completed).await;
    worker.abort();
    assert_eq!(status, Status::Completed);
}

#[tokio::test]
async fn removing_every_server_leaves_new_jobs_queued() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let lib = library();
    let mock = MockServer::start(lib.clone(), Config::default())
        .await
        .unwrap();
    let service = Service::new(runner(
        store.clone(),
        dir.path().join("dl"),
        vec![server(&mock)],
    ));
    service.set_usenet(Vec::new(), options());
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });
    let id = service
        .add_nzb("Stays", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    // Nothing can pick the job up; give the worker a moment to prove it doesn't.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(store.get(id).await.unwrap().status, Status::Queued);
    worker.abort();
}

struct Recorder {
    events: Arc<Mutex<Vec<NotificationEvent>>>,
}

impl Notifier for Recorder {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "notify-recorder".to_owned(),
            kind: ProviderKind::Notifier,
            description: "test".to_owned(),
        }
    }

    fn notify(&self, event: NotificationEvent) -> BoxFuture<Result<(), ProviderError>> {
        self.events.lock().unwrap().push(event);
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn notifiers_set_while_running_hear_about_the_next_job() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let lib = library();
    let mock = MockServer::start(lib.clone(), Config::default())
        .await
        .unwrap();
    let service = Service::new(runner(
        store.clone(),
        dir.path().join("dl"),
        vec![server(&mock)],
    ));
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    service.set_notifications(Notifications {
        notifiers: vec![Arc::new(Recorder {
            events: Arc::clone(&events),
        })],
        timeout: WAIT,
        min_free_bytes: 0,
        disk_check_interval: Duration::from_hours(1),
    });

    let id = service
        .add_nzb("Heard", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + WAIT;
    while events.lock().unwrap().is_empty() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    worker.abort();
    assert_eq!(
        events.lock().unwrap().clone(),
        [NotificationEvent::JobCompleted {
            job_id: id,
            name: "Heard".to_owned(),
            category: None,
        }]
    );
}

#[tokio::test]
async fn jobs_in_a_category_with_a_folder_download_into_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let lib = library();
    let mock = MockServer::start(lib.clone(), Config::default())
        .await
        .unwrap();
    let root = dir.path().join("dl");
    let elsewhere = dir.path().join("films");
    store.set_category_folders(HashMap::from([
        ("tv".to_owned(), PathBuf::from("TV")),
        ("movies".to_owned(), elsewhere.clone()),
    ]));
    let service = Service::new(runner(store.clone(), root.clone(), vec![server(&mock)]));
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });

    let tv = service
        .add_nzb("Show", Some("TV"), 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let movie = service
        .add_nzb("Film", Some("movies"), 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let plain = service
        .add_nzb("Plain", Some("other"), 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    for id in [tv, movie, plain] {
        assert_eq!(
            wait_for_status(&store, id, Status::Completed).await,
            Status::Completed
        );
    }
    worker.abort();
    let folder = async |id| store.get(id).await.unwrap().output_path.unwrap();
    assert!(
        folder(tv).await.starts_with(root.join("TV")),
        "names match ignoring case"
    );
    assert!(folder(movie).await.starts_with(&elsewhere));
    assert_eq!(folder(plain).await.parent(), Some(root.as_path()));
}

#[tokio::test]
async fn a_created_category_can_be_deleted_but_one_a_job_carries_stays_listed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    store.create_category("tv").await.unwrap();
    store.create_category("movies").await.unwrap();
    store
        .add_nzb("x", Some("movies"), 0, library().nzb().as_bytes())
        .await
        .unwrap();
    store.delete_category("tv").await.unwrap();
    store.delete_category("movies").await.unwrap();
    assert_eq!(store.categories().await.unwrap(), ["movies"]);
}
