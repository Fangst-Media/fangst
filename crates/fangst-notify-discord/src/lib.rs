//! Posts notifications to a Discord channel through a channel webhook.
//!
//! The webhook URL holds the token that lets anyone post to the channel, so it never appears in
//! errors.

use std::time::Duration;

use fangst_providers::{
    BoxFuture, NotificationEvent, Notifier, ProviderError, ProviderInfo, ProviderKind,
};
use reqwest::{StatusCode, Url};
use serde::Serialize;

pub const ID: &str = "notify-discord";

/// Discord rejects embeds whose description is longer.
const MAX_DESCRIPTION_CHARS: usize = 4096;
const GREEN: u32 = 0x002e_cc71;
const RED: u32 = 0x00e7_4c3c;
const ORANGE: u32 = 0x00e6_7e22;

pub struct Settings {
    /// `https://discord.com/api/webhooks/<id>/<token>`.
    pub webhook_url: String,
    pub timeout: Duration,
}

pub struct DiscordNotifier {
    client: reqwest::Client,
    url: Url,
}

#[derive(Serialize)]
struct Message {
    username: &'static str,
    embeds: [Embed; 1],
    allowed_mentions: AllowedMentions,
}

#[derive(Serialize)]
struct Embed {
    title: &'static str,
    description: String,
    color: u32,
}

#[derive(Serialize)]
struct AllowedMentions {
    parse: [&'static str; 0],
}

impl DiscordNotifier {
    /// # Errors
    /// When the URL is not an `http` or `https` URL, or the HTTP client can't be set up.
    pub fn new(settings: &Settings) -> Result<Self, ProviderError> {
        let url = Url::parse(&settings.webhook_url)
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .ok_or_else(|| {
                ProviderError::new(
                    "the Discord webhook URL must start with http:// or https://",
                    false,
                )
            })?;
        let client = reqwest::Client::builder()
            .timeout(settings.timeout)
            .user_agent(concat!("fangst/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ProviderError::new(format!("could not set up HTTP: {e}"), false))?;
        Ok(Self { client, url })
    }
}

impl Notifier for DiscordNotifier {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: ID.to_owned(),
            kind: ProviderKind::Notifier,
            description: "Posts events to a Discord channel".to_owned(),
        }
    }

    fn notify(&self, event: NotificationEvent) -> BoxFuture<Result<(), ProviderError>> {
        let color = match event {
            NotificationEvent::JobCompleted { .. } => GREEN,
            NotificationEvent::JobFailed { .. } => RED,
            NotificationEvent::DiskFull { .. } => ORANGE,
        };
        let message = Message {
            username: "Fangst",
            embeds: [Embed {
                title: event.title(),
                description: shorten(&event.message(), MAX_DESCRIPTION_CHARS),
                color,
            }],
            // Job names come from releases and could contain @everyone.
            allowed_mentions: AllowedMentions { parse: [] },
        };
        let request = self.client.post(self.url.clone()).json(&message);
        Box::pin(async move {
            let response = request.send().await.map_err(|e| {
                ProviderError::new(
                    format!("could not reach Discord: {}", e.without_url()),
                    true,
                )
            })?;
            check(response.status())
        })
    }
}

fn shorten(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(max_chars - 1).collect();
    short.push('…');
    short
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
        format!("Discord answered {status}"),
        retryable,
    ))
}
