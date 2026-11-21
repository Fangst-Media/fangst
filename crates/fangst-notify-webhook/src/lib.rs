//! Posts each notification as JSON to one URL, for scripts and services that take webhooks.
//!
//! The body is the event's own fields (`event`, `job_id`, `name`, ...) plus a ready-made `title`
//! and `message`.

use std::time::Duration;

use fangst_providers::{
    BoxFuture, NotificationEvent, Notifier, ProviderError, ProviderInfo, ProviderKind,
};
use reqwest::{StatusCode, Url};
use serde::Serialize;

pub const ID: &str = "notify-webhook";

pub struct Settings {
    /// Where events are posted; `http` or `https`.
    pub url: String,
    /// Sent as `Authorization: Bearer <token>` when set.
    pub bearer_token: Option<String>,
    pub timeout: Duration,
}

pub struct WebhookNotifier {
    client: reqwest::Client,
    url: Url,
    bearer_token: Option<String>,
}

#[derive(Serialize)]
struct Payload<'a> {
    #[serde(flatten)]
    event: &'a NotificationEvent,
    title: &'static str,
    message: String,
}

impl WebhookNotifier {
    /// # Errors
    /// When the URL is not an `http` or `https` URL, or the HTTP client can't be set up.
    pub fn new(settings: Settings) -> Result<Self, ProviderError> {
        let url = Url::parse(&settings.url)
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .ok_or_else(|| {
                ProviderError::new("the webhook URL must start with http:// or https://", false)
            })?;
        let client = reqwest::Client::builder()
            .timeout(settings.timeout)
            .user_agent(concat!("fangst/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ProviderError::new(format!("could not set up HTTP: {e}"), false))?;
        Ok(Self {
            client,
            url,
            bearer_token: settings.bearer_token,
        })
    }
}

impl Notifier for WebhookNotifier {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: ID.to_owned(),
            kind: ProviderKind::Notifier,
            description: "Posts each event as JSON to a URL".to_owned(),
        }
    }

    fn notify(&self, event: NotificationEvent) -> BoxFuture<Result<(), ProviderError>> {
        let payload = Payload {
            event: &event,
            title: event.title(),
            message: event.message(),
        };
        let mut request = self.client.post(self.url.clone()).json(&payload);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        Box::pin(async move {
            // The URL may hold a token, so it is left out of every error.
            let response = request.send().await.map_err(|e| {
                ProviderError::new(
                    format!("could not reach the webhook: {}", e.without_url()),
                    true,
                )
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
        format!("the webhook answered {status}"),
        retryable,
    ))
}
