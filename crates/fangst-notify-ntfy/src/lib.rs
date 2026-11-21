//! Publishes notifications to an ntfy topic, on ntfy.sh or a self-hosted server.
//!
//! Uses ntfy's JSON publishing (a POST to the server root with the topic in the body) rather than
//! headers, because HTTP headers can't carry job names with non-ASCII characters.

use std::time::Duration;

use fangst_providers::{
    BoxFuture, NotificationEvent, Notifier, ProviderError, ProviderInfo, ProviderKind,
};
use reqwest::{StatusCode, Url};
use serde::Serialize;

pub const ID: &str = "notify-ntfy";

/// ntfy refuses longer topic names.
const MAX_TOPIC_CHARS: usize = 64;

pub struct Settings {
    /// The ntfy server, e.g. `https://ntfy.sh`.
    pub server: String,
    pub topic: String,
    /// An ntfy access token, for protected topics.
    pub token: Option<String>,
    pub timeout: Duration,
}

pub struct NtfyNotifier {
    client: reqwest::Client,
    server: Url,
    topic: String,
    token: Option<String>,
}

#[derive(Serialize)]
struct Publish<'a> {
    topic: &'a str,
    title: &'static str,
    message: String,
    tags: [&'static str; 1],
    /// ntfy's scale: 1 min, 3 default, 5 max.
    priority: u8,
}

impl NtfyNotifier {
    /// # Errors
    /// When the server is not an `http` or `https` URL, the topic has characters ntfy refuses, or
    /// the HTTP client can't be set up.
    pub fn new(settings: Settings) -> Result<Self, ProviderError> {
        let server = Url::parse(&settings.server)
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .ok_or_else(|| {
                ProviderError::new("the ntfy server must start with http:// or https://", false)
            })?;
        let topic_ok = !settings.topic.is_empty()
            && settings.topic.len() <= MAX_TOPIC_CHARS
            && settings
                .topic
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !topic_ok {
            return Err(ProviderError::new(
                "an ntfy topic is 1 to 64 letters, digits, - or _",
                false,
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(settings.timeout)
            .user_agent(concat!("fangst/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ProviderError::new(format!("could not set up HTTP: {e}"), false))?;
        Ok(Self {
            client,
            server,
            topic: settings.topic,
            token: settings.token,
        })
    }
}

impl Notifier for NtfyNotifier {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: ID.to_owned(),
            kind: ProviderKind::Notifier,
            description: "Publishes events to an ntfy topic".to_owned(),
        }
    }

    fn notify(&self, event: NotificationEvent) -> BoxFuture<Result<(), ProviderError>> {
        let (tag, priority) = match event {
            NotificationEvent::JobCompleted { .. } => ("white_check_mark", 3),
            NotificationEvent::JobFailed { .. } => ("x", 4),
            NotificationEvent::DiskFull { .. } => ("warning", 4),
        };
        let publish = Publish {
            topic: &self.topic,
            title: event.title(),
            message: event.message(),
            tags: [tag],
            priority,
        };
        let mut request = self.client.post(self.server.clone()).json(&publish);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        Box::pin(async move {
            let response = request.send().await.map_err(|e| {
                ProviderError::new(format!("could not reach ntfy: {}", e.without_url()), true)
            })?;
            check(response.status())
        })
    }
}

fn check(status: StatusCode) -> Result<(), ProviderError> {
    if status.is_success() {
        return Ok(());
    }
    let retryable = status.is_server_error()
        || matches!(
            status,
            StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT
        );
    Err(ProviderError::new(
        format!("ntfy answered {status}"),
        retryable,
    ))
}
