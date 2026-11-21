use std::path::PathBuf;
use std::time::Duration;

use fangst_bench::http_sink::HttpSink;
use fangst_notify_ntfy::{NtfyNotifier, Settings};
use fangst_providers::{NotificationEvent, Notifier, ProviderKind};

const WAIT: Duration = Duration::from_secs(5);

fn settings(server: String, topic: &str, token: Option<&str>) -> Settings {
    Settings {
        server,
        topic: topic.to_owned(),
        token: token.map(str::to_owned),
        timeout: WAIT,
    }
}

fn events() -> Vec<NotificationEvent> {
    vec![
        NotificationEvent::JobCompleted {
            job_id: 1,
            name: "Done.Release".to_owned(),
            category: None,
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
async fn publishes_every_event_as_json_to_the_topic() {
    let sink = HttpSink::start(200).await.unwrap();
    let ntfy = NtfyNotifier::new(settings(sink.url("/"), "fangst-alerts", None)).unwrap();
    for event in events() {
        ntfy.notify(event).await.unwrap();
    }

    let requests = sink.wait_for(3, WAIT).await;
    assert_eq!(requests.len(), 3);
    let mut priorities = Vec::new();
    let mut tags = Vec::new();
    for (request, event) in requests.iter().zip(events()) {
        assert_eq!(request.method, "POST");
        // JSON publishing goes to the server root, with the topic in the body.
        assert_eq!(request.path, "/");
        assert_eq!(request.header("authorization"), None);
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["topic"], "fangst-alerts");
        assert_eq!(body["title"], event.title());
        assert_eq!(body["message"], event.message());
        priorities.push(body["priority"].as_i64().unwrap());
        tags.push(body["tags"][0].as_str().unwrap().to_owned());
    }
    assert_eq!(priorities, [3, 4, 4]);
    assert_eq!(tags, ["white_check_mark", "x", "warning"]);
}

#[tokio::test]
async fn server_with_a_path_keeps_it() {
    let sink = HttpSink::start(200).await.unwrap();
    NtfyNotifier::new(settings(sink.url("/ntfy/"), "t", None))
        .unwrap()
        .notify(events().remove(0))
        .await
        .unwrap();
    assert_eq!(sink.wait_for(1, WAIT).await[0].path, "/ntfy/");
}

#[tokio::test]
async fn sends_the_access_token() {
    let sink = HttpSink::start(200).await.unwrap();
    NtfyNotifier::new(settings(sink.url(""), "t", Some("tk_abc")))
        .unwrap()
        .notify(events().remove(0))
        .await
        .unwrap();
    let requests = sink.wait_for(1, WAIT).await;
    assert_eq!(requests[0].header("authorization"), Some("Bearer tk_abc"));
}

#[tokio::test]
async fn refused_publish_is_an_error_without_the_token() {
    let sink = HttpSink::start(403).await.unwrap();
    let err = NtfyNotifier::new(settings(sink.url("/"), "t", Some("tk_abc")))
        .unwrap()
        .notify(events().remove(0))
        .await
        .unwrap_err();
    assert!(!err.retryable);
    assert!(err.message.contains("403"), "{err}");
    assert!(!err.message.contains("tk_abc"), "{err}");
}

#[test]
fn refuses_bad_topics_and_servers() {
    for topic in ["", "has space", "a/b", &"x".repeat(65)] {
        assert!(
            NtfyNotifier::new(settings("https://ntfy.sh".to_owned(), topic, None)).is_err(),
            "{topic:?}"
        );
    }
    assert!(NtfyNotifier::new(settings("ntfy.sh".to_owned(), "t", None)).is_err());
}

#[test]
fn describes_itself() {
    let ntfy = NtfyNotifier::new(settings("https://ntfy.sh".to_owned(), "t", None)).unwrap();
    assert_eq!(ntfy.info().id, "notify-ntfy");
    assert_eq!(ntfy.info().kind, ProviderKind::Notifier);
}
