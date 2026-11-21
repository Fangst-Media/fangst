use std::path::PathBuf;
use std::time::Duration;

use fangst_bench::http_sink::HttpSink;
use fangst_notify_discord::{DiscordNotifier, Settings};
use fangst_providers::{NotificationEvent, Notifier, ProviderKind};

const WAIT: Duration = Duration::from_secs(5);

fn discord(url: String) -> DiscordNotifier {
    DiscordNotifier::new(&Settings {
        webhook_url: url,
        timeout: WAIT,
    })
    .unwrap()
}

fn events() -> Vec<NotificationEvent> {
    vec![
        NotificationEvent::JobCompleted {
            job_id: 1,
            name: "Done.Release".to_owned(),
            category: Some("movies".to_owned()),
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
async fn posts_every_event_as_an_embed() {
    // Discord answers a webhook post with 204 No Content.
    let sink = HttpSink::start(204).await.unwrap();
    let hook = discord(sink.url("/api/webhooks/123/token"));
    for event in events() {
        hook.notify(event).await.unwrap();
    }

    let requests = sink.wait_for(3, WAIT).await;
    assert_eq!(requests.len(), 3);
    let mut colors = Vec::new();
    for (request, event) in requests.iter().zip(events()) {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/api/webhooks/123/token");
        assert_eq!(request.header("content-type"), Some("application/json"));
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["username"], "Fangst");
        let embed = &body["embeds"][0];
        assert_eq!(embed["title"], event.title());
        assert_eq!(embed["description"], event.message());
        colors.push(embed["color"].as_u64().unwrap());
        // A job named "@everyone" must not ping the channel.
        assert_eq!(body["allowed_mentions"]["parse"], serde_json::json!([]));
    }
    assert_eq!(colors.len(), 3);
    assert_ne!(colors[0], colors[1]);
}

#[tokio::test]
async fn long_messages_are_cut_to_discords_limit() {
    let sink = HttpSink::start(204).await.unwrap();
    discord(sink.url("/"))
        .notify(NotificationEvent::JobFailed {
            job_id: 1,
            name: "x".to_owned(),
            reason: "é".repeat(5000),
        })
        .await
        .unwrap();
    let requests = sink.wait_for(1, WAIT).await;
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let description = body["embeds"][0]["description"].as_str().unwrap();
    assert_eq!(description.chars().count(), 4096);
    assert!(description.ends_with('…'));
}

#[tokio::test]
async fn rate_limit_is_retryable_and_hides_the_webhook_token() {
    let sink = HttpSink::start(429).await.unwrap();
    let err = discord(sink.url("/api/webhooks/123/s3cret"))
        .notify(events().remove(0))
        .await
        .unwrap_err();
    assert!(err.retryable);
    assert!(err.message.contains("429"), "{err}");
    assert!(!err.message.contains("s3cret"), "{err}");
}

#[test]
fn refuses_urls_that_are_not_http() {
    let result = DiscordNotifier::new(&Settings {
        webhook_url: "discord.com/api/webhooks/1/x".to_owned(),
        timeout: WAIT,
    });
    assert!(result.is_err());
}

#[test]
fn describes_itself() {
    let info = discord("https://discord.com/api/webhooks/1/x".to_owned()).info();
    assert_eq!(info.id, "notify-discord");
    assert_eq!(info.kind, ProviderKind::Notifier);
}
