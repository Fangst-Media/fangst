use std::path::PathBuf;
use std::time::Duration;

use fangst_bench::http_sink::HttpSink;
use fangst_notify_webhook::{Settings, WebhookNotifier};
use fangst_providers::{NotificationEvent, Notifier, ProviderKind};

const WAIT: Duration = Duration::from_secs(5);

fn notifier(url: String, token: Option<&str>) -> WebhookNotifier {
    WebhookNotifier::new(Settings {
        url,
        bearer_token: token.map(str::to_owned),
        timeout: WAIT,
    })
    .unwrap()
}

fn events() -> Vec<NotificationEvent> {
    vec![
        NotificationEvent::JobCompleted {
            job_id: 1,
            name: "Done.Release".to_owned(),
            category: Some("tv".to_owned()),
        },
        NotificationEvent::JobFailed {
            job_id: 2,
            name: "Broken.Release".to_owned(),
            reason: "3 segments missing".to_owned(),
        },
        NotificationEvent::DiskFull {
            path: PathBuf::from("downloads"),
            free_bytes: 1024,
        },
    ]
}

#[tokio::test]
async fn posts_every_event_as_json() {
    let sink = HttpSink::start(200).await.unwrap();
    let webhook = notifier(sink.url("/hook?source=fangst"), None);
    for event in events() {
        webhook.notify(event).await.unwrap();
    }

    let requests = sink.wait_for(3, WAIT).await;
    assert_eq!(requests.len(), 3);
    for (request, event) in requests.iter().zip(events()) {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/hook?source=fangst");
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.header("authorization"), None);
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["event"], event.name());
        assert_eq!(body["title"], event.title());
        assert_eq!(body["message"], event.message());
    }
    let completed: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(completed["job_id"], 1);
    assert_eq!(completed["name"], "Done.Release");
    assert_eq!(completed["category"], "tv");
    let failed: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(failed["reason"], "3 segments missing");
    let disk: serde_json::Value = serde_json::from_slice(&requests[2].body).unwrap();
    assert_eq!(disk["free_bytes"], 1024);
}

#[tokio::test]
async fn sends_the_bearer_token() {
    let sink = HttpSink::start(204).await.unwrap();
    notifier(sink.url("/"), Some("s3cret"))
        .notify(events().remove(0))
        .await
        .unwrap();
    let requests = sink.wait_for(1, WAIT).await;
    assert_eq!(requests[0].header("authorization"), Some("Bearer s3cret"));
}

#[tokio::test]
async fn error_status_fails_without_leaking_the_url() {
    let sink = HttpSink::start(500).await.unwrap();
    let err = notifier(sink.url("/hook?token=s3cret"), None)
        .notify(events().remove(0))
        .await
        .unwrap_err();
    assert!(err.retryable);
    assert!(err.message.contains("500"), "{err}");
    assert!(!err.message.contains("s3cret"), "{err}");

    let sink = HttpSink::start(404).await.unwrap();
    let err = notifier(sink.url("/"), None)
        .notify(events().remove(0))
        .await
        .unwrap_err();
    assert!(!err.retryable);
}

#[tokio::test]
async fn unreachable_url_fails_without_leaking_it() {
    let sink = HttpSink::start(200).await.unwrap();
    let url = sink.url("/hook?token=s3cret");
    drop(sink);
    let err = notifier(url, None)
        .notify(events().remove(0))
        .await
        .unwrap_err();
    assert!(err.retryable);
    assert!(!err.message.contains("s3cret"), "{err}");
}

#[test]
fn refuses_urls_that_are_not_http() {
    for url in ["ftp://example.com/", "not a url", "file:///etc/passwd"] {
        let result = WebhookNotifier::new(Settings {
            url: url.to_owned(),
            bearer_token: None,
            timeout: WAIT,
        });
        assert!(result.is_err(), "{url}");
    }
}

#[test]
fn describes_itself() {
    let info = notifier("http://127.0.0.1/".to_owned(), None).info();
    assert_eq!(info.id, fangst_notify_webhook::ID);
    assert_eq!(info.id, "notify-webhook");
    assert_eq!(info.kind, ProviderKind::Notifier);
}
