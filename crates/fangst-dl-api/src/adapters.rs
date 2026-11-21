//! Mounts enabled API adapters next to the native API and gives them the job operations.
//!
//! An adapter reads its own credential from the request (`apikey=` for SABnzbd), and this module
//! checks it against the same user API keys as `X-Api-Key`. Session cookies never count here:
//! adapter routes take state-changing GETs, which a cookie would let any page trigger.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use fangst_downloads::{Error as QueueError, Job, Kind, Service, Status};
use fangst_providers::{
    AddNzb, AddNzbUrl, ApiAdapter, BoxFuture, Credential, HttpRequest, HttpResponse, JobSummary,
    Jobs, ProviderError,
};

use crate::fetch::{FetchError, Fetcher};
use crate::{ApiState, MAX_NZB_BYTES};

/// The download service as the job operations adapters translate onto.
#[derive(Clone)]
pub(crate) struct ServiceJobs {
    pub service: Service,
    pub fetcher: Fetcher,
}

pub(crate) fn summary(job: Job) -> JobSummary {
    JobSummary {
        id: job.id,
        kind: match job.kind {
            Kind::Usenet => "usenet",
            Kind::Torrent => "torrent",
        }
        .to_owned(),
        name: job.name,
        category: job.category,
        priority: job.priority,
        status: match job.status {
            Status::Queued => "queued",
            Status::Downloading => "downloading",
            Status::Paused => "paused",
            Status::Completed => "completed",
            Status::Failed => "failed",
        }
        .to_owned(),
        error: job.error,
        output_path: job.output_path,
        total_bytes: job.total_bytes,
        downloaded_bytes: job.downloaded_bytes,
        rate_bytes_per_sec: job.rate_bytes_per_sec,
        eta_seconds: job.eta_seconds,
        added_at: job.added_at,
        finished_at: job.finished_at,
    }
}

fn provider_error(e: &QueueError) -> ProviderError {
    match e {
        QueueError::NotFound(_)
        | QueueError::InvalidNzb(_)
        | QueueError::InvalidTorrent(_)
        | QueueError::InvalidState { .. }
        | QueueError::DuplicateTorrent(_)
        | QueueError::Invalid(_) => ProviderError::new(e.to_string(), false),
        QueueError::Busy => ProviderError::new(e.to_string(), true),
        _ => {
            eprintln!("error: {e}");
            ProviderError::new("internal error; the daemon's log has the details", true)
        }
    }
}

pub(crate) fn fetch_error(e: &FetchError) -> ProviderError {
    let retryable = matches!(e, FetchError::Network(_) | FetchError::Status(_));
    ProviderError::new(e.to_string(), retryable)
}

impl ServiceJobs {
    async fn queue(
        &self,
        name: Option<String>,
        fallback_name: Option<String>,
        category: Option<String>,
        priority: i64,
        nzb: Vec<u8>,
    ) -> Result<JobSummary, ProviderError> {
        let name = name
            .filter(|n| !n.trim().is_empty())
            .or_else(|| crate::fangst_nzb_title(&nzb))
            .or(fallback_name)
            .unwrap_or_else(|| "Untitled".to_owned());
        let id = self
            .service
            .add_nzb(&name, category.as_deref(), priority, &nzb)
            .await
            .map_err(|e| provider_error(&e))?;
        let job = self
            .service
            .store()
            .get(id)
            .await
            .map_err(|e| provider_error(&e))?;
        Ok(summary(job))
    }
}

impl Jobs for ServiceJobs {
    fn add_nzb(&self, request: AddNzb) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let this = self.clone();
        Box::pin(async move {
            this.queue(
                request.name,
                None,
                request.category,
                request.priority,
                request.nzb,
            )
            .await
        })
    }

    fn add_nzb_url(&self, request: AddNzbUrl) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let this = self.clone();
        Box::pin(async move {
            let fetched = this
                .fetcher
                .fetch(&request.url)
                .await
                .map_err(|e| fetch_error(&e))?;
            this.queue(
                request.name,
                fetched.file_name,
                request.category,
                request.priority,
                fetched.bytes,
            )
            .await
        })
    }

    fn list(&self) -> BoxFuture<Result<Vec<JobSummary>, ProviderError>> {
        let service = self.service.clone();
        Box::pin(async move {
            let jobs = service
                .store()
                .list()
                .await
                .map_err(|e| provider_error(&e))?;
            Ok(jobs.into_iter().map(summary).collect())
        })
    }

    fn pause(&self, job_id: i64) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let service = self.service.clone();
        Box::pin(async move {
            service
                .pause(job_id)
                .await
                .map(summary)
                .map_err(|e| provider_error(&e))
        })
    }

    fn resume(&self, job_id: i64) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let service = self.service.clone();
        Box::pin(async move {
            service
                .resume(job_id)
                .await
                .map(summary)
                .map_err(|e| provider_error(&e))
        })
    }

    fn delete(&self, job_id: i64, delete_files: bool) -> BoxFuture<Result<(), ProviderError>> {
        let service = self.service.clone();
        Box::pin(async move {
            service
                .delete(job_id, delete_files)
                .await
                .map_err(|e| provider_error(&e))
        })
    }

    fn download_root(&self) -> std::path::PathBuf {
        let root = self.service.download_root();
        // Clients compare it with paths on their side, which only works when it is absolute.
        std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf())
    }
}

/// Routes `mount_path` and everything below it to `adapter`.
pub(crate) fn mount(router: Router<ApiState>, adapter: &Arc<dyn ApiAdapter>) -> Router<ApiState> {
    let mount = adapter.mount_path();
    let exact = Arc::clone(adapter);
    let below = Arc::clone(adapter);
    router
        .route(
            &mount,
            any(move |State(state): State<ApiState>, request: Request| {
                let adapter = Arc::clone(&exact);
                async move { serve(state, adapter, request).await }
            }),
        )
        .route(
            &format!("{mount}/{{*rest}}"),
            any(move |State(state): State<ApiState>, request: Request| {
                let adapter = Arc::clone(&below);
                async move { serve(state, adapter, request).await }
            }),
        )
}

async fn serve(state: ApiState, adapter: Arc<dyn ApiAdapter>, request: Request) -> Response {
    let mount = adapter.mount_path();
    let (parts, body) = request.into_parts();
    let Ok(body) = axum::body::to_bytes(body, MAX_NZB_BYTES).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let path = parts.uri.path().strip_prefix(mount.as_str()).unwrap_or("");
    let request = HttpRequest {
        method: parts.method.as_str().to_owned(),
        path: if path.is_empty() {
            "/".to_owned()
        } else {
            path.to_owned()
        },
        query: parts
            .uri
            .query()
            .map(|q| form_urlencoded::parse(q.as_bytes()).into_owned().collect())
            .unwrap_or_default(),
        headers: parts
            .headers
            .iter()
            .filter_map(|(name, value)| {
                Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
            })
            .collect(),
        body: body.to_vec(),
    };
    let key = match adapter.credential(&request) {
        Credential::NotNeeded => None,
        Credential::ApiKey(key) => Some(key),
        Credential::Missing => return into_response(adapter.rejected(&request, true)),
    };
    if let Some(key) = key {
        match state.auth.user_for_api_key(&key).await {
            Ok(Some(_)) => {}
            Ok(None) => return into_response(adapter.rejected(&request, false)),
            Err(e) => return crate::internal(&e),
        }
    }
    let jobs: Arc<dyn Jobs> = Arc::new(state.jobs.clone());
    into_response(adapter.handle(request, jobs).await)
}

fn into_response(response: HttpResponse) -> Response {
    let mut out = Response::new(Body::from(response.body));
    *out.status_mut() =
        StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    for (name, value) in response.headers {
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(name), HeaderValue::try_from(value)) {
            out.headers_mut().append(name, value);
        }
    }
    out
}
