use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_downloads::{Event, Notifications, Runner, Service, Status, Store};
use fangst_nntp::{Pool, ServerConfig};
use fangst_providers::{
    BoxFuture, NotificationEvent, Notifier, ProviderError, ProviderInfo, ProviderKind,
};

const WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
enum Behaviour {
    Succeed,
    Fail,
    Hang,
}

struct Recorder {
    id: &'static str,
    behaviour: Behaviour,
    events: Arc<Mutex<Vec<NotificationEvent>>>,
}

impl Recorder {
    fn new(
        id: &'static str,
        behaviour: Behaviour,
    ) -> (Arc<Self>, Arc<Mutex<Vec<NotificationEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::new(Self {
            id,
            behaviour,
            events: Arc::clone(&events),
        });
        (recorder, events)
    }
}

impl Notifier for Recorder {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.id.to_owned(),
            kind: ProviderKind::Notifier,
            description: "test".to_owned(),
        }
    }

    fn notify(&self, event: NotificationEvent) -> BoxFuture<Result<(), ProviderError>> {
        self.events.lock().unwrap().push(event);
        let behaviour = self.behaviour;
        Box::pin(async move {
            match behaviour {
                Behaviour::Succeed => Ok(()),
                Behaviour::Fail => Err(ProviderError::new("the sink said no", true)),
                Behaviour::Hang => std::future::pending().await,
            }
        })
    }
}

fn library() -> Library {
    let file: Vec<u8> = (0..20_000u32).map(|i| i.to_le_bytes()[0]).collect();
    let mut library = Library::new();
    library.add_file("payload.bin", &file, 5000, "alt.binaries.test");
    library
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
        post_processors: Vec::new(),
    }
}

fn notifications(notifiers: Vec<Arc<dyn Notifier>>, timeout: Duration) -> Notifications {
    Notifications {
        notifiers,
        timeout,
        // Real disks have less than this, so the disk check stays quiet unless a test lowers it.
        min_free_bytes: 0,
        disk_check_interval: Duration::from_hours(1),
    }
}

async fn wait_until<F: Fn(&[Event]) -> bool>(store: &Store, id: i64, done: F) -> Vec<Event> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let events = store.timeline(id).await.unwrap();
        if done(&events) || tokio::time::Instant::now() >= deadline {
            return events;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn notify_entries(events: &[Event]) -> Vec<&str> {
    events
        .iter()
        .filter(|e| e.kind == "notify")
        .map(|e| e.message.as_str())
        .collect()
}

#[tokio::test]
async fn completed_job_is_announced_and_a_failing_notifier_only_leaves_a_timeline_entry() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let lib = library();
    let server = MockServer::start(lib.clone(), Config::default())
        .await
        .unwrap();
    let (good, sent) = Recorder::new("notify-good", Behaviour::Succeed);
    let (bad, _) = Recorder::new("notify-bad", Behaviour::Fail);
    let service = Service::new(runner(store.clone(), &server, dir.path()))
        .with_notifications(notifications(vec![good, bad], WAIT));
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });

    let id = service
        .add_nzb("Some.Release", Some("tv"), 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let events = wait_until(&store, id, |e| !notify_entries(e).is_empty()).await;
    worker.abort();

    assert_eq!(store.get(id).await.unwrap().status, Status::Completed);
    assert_eq!(
        notify_entries(&events),
        ["notify-bad could not send the job_completed notification: the sink said no"]
    );
    // The good notifier ran too, and nothing was recorded for it.
    let sent = sent.lock().unwrap().clone();
    assert_eq!(
        sent,
        [NotificationEvent::JobCompleted {
            job_id: id,
            name: "Some.Release".to_owned(),
            category: Some("tv".to_owned()),
        }]
    );
}

#[tokio::test]
async fn failed_job_is_announced_with_its_reason() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let lib = library();
    let missing: HashSet<String> = lib.message_ids().into_iter().take(1).collect();
    let config = Config {
        missing,
        ..Config::default()
    };
    let server = MockServer::start(lib.clone(), config).await.unwrap();
    let (recorder, sent) = Recorder::new("notify-good", Behaviour::Succeed);
    let service = Service::new(runner(store.clone(), &server, dir.path()))
        .with_notifications(notifications(vec![recorder], WAIT));
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });

    let id = service
        .add_nzb("Holes", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + WAIT;
    while sent.lock().unwrap().is_empty() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    worker.abort();

    let job = store.get(id).await.unwrap();
    assert_eq!(job.status, Status::Failed);
    let sent = sent.lock().unwrap().clone();
    assert_eq!(
        sent,
        [NotificationEvent::JobFailed {
            job_id: id,
            name: "Holes".to_owned(),
            reason: job.error.unwrap(),
        }]
    );
}

#[tokio::test]
async fn a_hanging_notifier_times_out_without_holding_up_the_queue() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let lib = library();
    let server = MockServer::start(lib.clone(), Config::default())
        .await
        .unwrap();
    let (slow, _) = Recorder::new("notify-slow", Behaviour::Hang);
    let service = Service::new(runner(store.clone(), &server, dir.path()))
        .with_notifications(notifications(vec![slow], Duration::from_millis(300)));
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });

    let first = service
        .add_nzb("First", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let second = service
        .add_nzb("Second", None, 0, lib.nzb().as_bytes())
        .await
        .unwrap();
    let events = wait_until(&store, first, |e| !notify_entries(e).is_empty()).await;
    let second_events = wait_until(&store, second, |e| !notify_entries(e).is_empty()).await;
    worker.abort();

    assert_eq!(store.get(first).await.unwrap().status, Status::Completed);
    assert_eq!(store.get(second).await.unwrap().status, Status::Completed);
    assert_eq!(
        notify_entries(&events),
        ["notify-slow did not send the job_completed notification within 0.3 s"]
    );
    assert_eq!(notify_entries(&second_events).len(), 1);
}

#[tokio::test]
async fn low_disk_space_is_announced_once_per_crossing() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let server = MockServer::start(library(), Config::default())
        .await
        .unwrap();
    let (recorder, sent) = Recorder::new("notify-good", Behaviour::Succeed);
    let root = dir.path().join("downloads");
    let service =
        Service::new(runner(store.clone(), &server, &root)).with_notifications(Notifications {
            // No disk has this much free, so every check finds it below the threshold.
            min_free_bytes: u64::MAX,
            disk_check_interval: Duration::from_millis(20),
            ..notifications(vec![recorder], WAIT)
        });
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });
    tokio::time::sleep(Duration::from_millis(400)).await;
    worker.abort();

    let sent = sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "{sent:?}");
    let NotificationEvent::DiskFull { path, free_bytes } = &sent[0] else {
        panic!("{sent:?}");
    };
    // The downloads folder doesn't exist before the first job; its parent is measured instead.
    assert_eq!(path, &root);
    assert!(*free_bytes > 0);
}
