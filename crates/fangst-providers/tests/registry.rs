use std::sync::{Arc, Mutex};

use fangst_providers::{
    BoxFuture, JobContext, NotificationEvent, Notifier, PostProcessOutcome, PostProcessor,
    Provider, ProviderError, ProviderInfo, ProviderKind, Registry, RegistryError,
};

struct RecordingNotifier {
    id: &'static str,
    seen: Arc<Mutex<Vec<NotificationEvent>>>,
}

impl Notifier for RecordingNotifier {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.id.to_owned(),
            kind: ProviderKind::Notifier,
            description: "test".to_owned(),
        }
    }

    fn notify(&self, event: NotificationEvent) -> BoxFuture<Result<(), ProviderError>> {
        let seen = Arc::clone(&self.seen);
        Box::pin(async move {
            seen.lock().unwrap().push(event);
            Ok(())
        })
    }
}

struct Refuser;

impl PostProcessor for Refuser {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "refuse".to_owned(),
            kind: ProviderKind::PostProcessor,
            description: "test".to_owned(),
        }
    }

    fn process(&self, job: JobContext) -> BoxFuture<Result<PostProcessOutcome, ProviderError>> {
        Box::pin(async move {
            Ok(PostProcessOutcome::Fail {
                reason: format!("refused {}", job.name),
            })
        })
    }
}

fn available(seen: &Arc<Mutex<Vec<NotificationEvent>>>) -> Vec<Provider> {
    vec![
        Provider::Notifier(Arc::new(RecordingNotifier {
            id: "notify-test",
            seen: Arc::clone(seen),
        })),
        Provider::PostProcessor(Arc::new(Refuser)),
    ]
}

#[test]
fn nothing_is_enabled_unless_asked() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let registry = Registry::build(available(&seen), &[]).unwrap();
    assert_eq!(registry.enabled_count(), 0);
    let listing: Vec<(&str, bool)> = registry
        .listing()
        .iter()
        .map(|(i, on)| (i.id.as_str(), *on))
        .collect();
    assert_eq!(listing, [("notify-test", false), ("refuse", false)]);
}

#[tokio::test]
async fn enabled_providers_are_wired_and_callable() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let registry = Registry::build(
        available(&seen),
        &["notify-test".to_owned(), "refuse".to_owned()],
    )
    .unwrap();
    assert_eq!(
        (registry.notifiers.len(), registry.post_processors.len()),
        (1, 1)
    );

    let event = NotificationEvent::JobFailed {
        job_id: 7,
        name: "x".to_owned(),
        reason: "missing".to_owned(),
    };
    registry.notifiers[0].notify(event.clone()).await.unwrap();
    assert_eq!(*seen.lock().unwrap(), [event]);

    let job = JobContext {
        job_id: 7,
        name: "x".to_owned(),
        category: None,
        folder: "jobs/7".into(),
        password: None,
    };
    let outcome = registry.post_processors[0].process(job).await.unwrap();
    assert_eq!(
        outcome,
        PostProcessOutcome::Fail {
            reason: "refused x".to_owned()
        }
    );
}

#[test]
fn unknown_id_lists_what_is_available() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let err = Registry::build(available(&seen), &["notify-typo".to_owned()])
        .err()
        .unwrap();
    assert_eq!(
        err.to_string(),
        "unknown provider `notify-typo`; available: notify-test, refuse"
    );
    let none = Registry::build(Vec::new(), &["anything".to_owned()])
        .err()
        .unwrap();
    assert!(none.to_string().contains("no optional providers"));
}

#[test]
fn duplicate_ids_are_refused() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut providers = available(&seen);
    providers.push(Provider::PostProcessor(Arc::new(Refuser)));
    assert_eq!(
        Registry::build(providers, &[]).err(),
        Some(RegistryError::DuplicateId("refuse".to_owned()))
    );
}

#[test]
fn provider_data_is_plain_serde() {
    // A WebAssembly plugin will exchange exactly these shapes as JSON.
    let event = NotificationEvent::JobCompleted {
        job_id: 1,
        name: "n".to_owned(),
        category: Some("tv".to_owned()),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert_eq!(
        json,
        r#"{"event":"job_completed","job_id":1,"name":"n","category":"tv"}"#
    );
    assert_eq!(
        serde_json::from_str::<NotificationEvent>(&json).unwrap(),
        event
    );
    let outcome: PostProcessOutcome =
        serde_json::from_str(r#"{"outcome":"fail","reason":"r"}"#).unwrap();
    assert_eq!(
        outcome,
        PostProcessOutcome::Fail {
            reason: "r".to_owned()
        }
    );
}
