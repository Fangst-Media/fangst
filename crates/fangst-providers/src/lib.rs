//! Extension points of Fangst.
//!
//! Every extension point is a trait whose methods take and return plain serde data: no Rust
//! handles, references into Fangst internals or callbacks. That keeps built-in providers honest and
//! lets a WebAssembly plugin implement the same contract later (milestone 3) without redesign.
//!
//! Methods return boxed futures instead of using `async fn` so the traits stay usable as
//! `dyn Trait`, which a registry of providers needs.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

mod notify;
mod registry;
pub use registry::{Provider, Registry, RegistryError};

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Describes a provider so it can be listed, enabled and disabled by its `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Stable, unique, lowercase with dashes, e.g. `notify-ntfy`. Config refers to providers by it.
    pub id: String,
    pub kind: ProviderKind,
    /// One line for settings pages.
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    PostProcessor,
    Notifier,
    ApiAdapter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderError {
    /// What went wrong, in words a user can act on. Never contains secrets.
    pub message: String,
    /// Whether trying again later could work, as opposed to a configuration or input problem.
    pub retryable: bool,
}

impl ProviderError {
    #[must_use]
    pub fn new(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            message: message.into(),
            retryable,
        }
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProviderError {}

/// A finished download as a post-processor sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobContext {
    pub job_id: i64,
    pub name: String,
    pub category: Option<String>,
    /// The job's folder. Post-processors must not write outside it.
    pub folder: PathBuf,
    /// Archive password, from the NZB or the job.
    pub password: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PostProcessOutcome {
    /// Done; the next post-processor runs. The message goes on the job's timeline.
    Continue { message: String },
    /// The job fails with this reason and no later post-processor runs.
    Fail { reason: String },
}

/// Runs after a download finishes, in registry order: repair, unpack, safety checks, scripts.
pub trait PostProcessor: Send + Sync {
    fn info(&self) -> ProviderInfo;
    fn process(&self, job: JobContext) -> BoxFuture<Result<PostProcessOutcome, ProviderError>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum NotificationEvent {
    JobCompleted {
        job_id: i64,
        name: String,
        category: Option<String>,
    },
    JobFailed {
        job_id: i64,
        name: String,
        reason: String,
    },
    DiskFull {
        path: PathBuf,
        free_bytes: u64,
    },
}

/// Tells people about events (webhook, ntfy, Discord). A failed notification never fails a job.
pub trait Notifier: Send + Sync {
    fn info(&self) -> ProviderInfo;
    fn notify(&self, event: NotificationEvent) -> BoxFuture<Result<(), ProviderError>>;
}

/// An HTTP request as an adapter sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: String,
    /// Path below the adapter's mount point, starting with `/`.
    pub path: String,
    /// Decoded query parameters in the order they appeared.
    pub query: Vec<(String, String)>,
    /// Header names in lowercase.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    #[must_use]
    pub fn query(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    #[must_use]
    pub fn json(status: u16, body: &impl Serialize) -> Self {
        Self {
            status,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            body: serde_json::to_vec(body).unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn text(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: vec![(
                "content-type".to_owned(),
                "text/plain; charset=utf-8".to_owned(),
            )],
            body: body.into().into_bytes(),
        }
    }
}

/// Job operations an adapter translates onto. Implemented by the download service.
pub trait Jobs: Send + Sync {
    fn add_nzb(&self, request: AddNzb) -> BoxFuture<Result<JobSummary, ProviderError>>;
    /// Fetches an NZB over HTTP(S) and queues it.
    fn add_nzb_url(&self, request: AddNzbUrl) -> BoxFuture<Result<JobSummary, ProviderError>>;
    fn list(&self) -> BoxFuture<Result<Vec<JobSummary>, ProviderError>>;
    fn pause(&self, job_id: i64) -> BoxFuture<Result<JobSummary, ProviderError>>;
    fn resume(&self, job_id: i64) -> BoxFuture<Result<JobSummary, ProviderError>>;
    fn delete(&self, job_id: i64, delete_files: bool) -> BoxFuture<Result<(), ProviderError>>;
    /// The absolute folder job folders are created in; protocols report it as their output folder.
    fn download_root(&self) -> PathBuf;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddNzb {
    pub name: Option<String>,
    pub category: Option<String>,
    pub priority: i64,
    pub nzb: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddNzbUrl {
    /// `http` or `https` only.
    pub url: String,
    /// Defaults to the NZB's title, then to the file name in the URL.
    pub name: Option<String>,
    pub category: Option<String>,
    pub priority: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSummary {
    pub id: i64,
    /// `usenet` or `torrent`.
    pub kind: String,
    pub name: String,
    pub category: Option<String>,
    /// Higher runs first.
    pub priority: i64,
    /// `queued`, `downloading`, `paused`, `completed` or `failed`.
    pub status: String,
    pub error: Option<String>,
    pub output_path: Option<PathBuf>,
    pub total_bytes: i64,
    /// Bytes on disk; a completed job counts as `total_bytes`.
    pub downloaded_bytes: i64,
    /// Averaged over the last few seconds; 0 unless downloading.
    pub rate_bytes_per_sec: i64,
    /// Seconds left at the current rate, when known.
    pub eta_seconds: Option<i64>,
    pub added_at: i64,
    pub finished_at: Option<i64>,
}

/// What a request to an adapter carries to prove who sent it, read from wherever the protocol
/// puts it. The daemon checks an API key against the same user keys as its own API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Credential {
    /// The protocol answers this request without a credential, such as a version probe.
    NotNeeded,
    ApiKey(String),
    Missing,
}

/// Speaks another product's API (qBittorrent, SABnzbd, Sonarr) by translating onto [`Jobs`].
/// Adapters hold no business logic and no state beyond ID mapping.
pub trait ApiAdapter: Send + Sync {
    fn info(&self) -> ProviderInfo;
    /// Where the adapter's routes live, e.g. `/api` for SABnzbd. Starts with `/`, no trailing `/`.
    /// The daemon routes the path itself and everything below it, except its own `/api/v1`.
    fn mount_path(&self) -> String;
    /// Looked at before [`ApiAdapter::handle`]; only requests that pass reach it.
    fn credential(&self, request: &HttpRequest) -> Credential;
    /// The protocol's own answer to a request whose key is missing (`missing`) or wrong.
    fn rejected(&self, request: &HttpRequest, missing: bool) -> HttpResponse;
    fn handle(
        &self,
        request: HttpRequest,
        jobs: std::sync::Arc<dyn Jobs>,
    ) -> BoxFuture<HttpResponse>;
}
