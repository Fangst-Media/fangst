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
    AddNzb, AddNzbUrl, AddTorrent, ApiAdapter, BoxFuture, Credential, HttpRequest, HttpResponse,
    JobSummary, Jobs, LoginOutcome, ProviderError, TorrentFile, TorrentInput, TorrentStatus,
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
        // Clients compare it with paths on their side, which only works when it is absolute.
        output_path: job
            .output_path
            .map(|p| std::path::absolute(&p).unwrap_or(p)),
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
            if fetched.magnet.is_some() {
                return Err(ProviderError::new(
                    "the URL leads to a magnet link, not an NZB",
                    false,
                ));
            }
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

    fn add_torrent(&self, request: AddTorrent) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let this = self.clone();
        Box::pin(async move {
            let (input, file_name) = match request.input {
                TorrentInput::Url(url) => {
                    let fetched = this
                        .fetcher
                        .fetch(&url)
                        .await
                        .map_err(|e| fetch_error(&e))?;
                    if let Some(link) = fetched.magnet {
                        (TorrentInput::Magnet(link), None)
                    } else if fetched.is_torrent() {
                        (TorrentInput::File(fetched.bytes), fetched.file_name)
                    } else {
                        return Err(ProviderError::new(
                            "the URL answered with something that is not a torrent",
                            false,
                        ));
                    }
                }
                other => (other, None),
            };
            let source = match &input {
                TorrentInput::Magnet(link) => fangst_downloads::TorrentSource::Magnet(link),
                TorrentInput::File(bytes) => fangst_downloads::TorrentSource::File(bytes),
                TorrentInput::Url(_) => unreachable!("fetched above"),
            };
            let id = crate::add_torrent(
                &this.service,
                request.name,
                file_name,
                request.category.as_deref(),
                request.priority,
                source,
            )
            .await
            .map_err(|e| provider_error(&e))?;
            let job = this
                .service
                .store()
                .get(id)
                .await
                .map_err(|e| provider_error(&e))?;
            Ok(summary(job))
        })
    }

    fn torrent(&self, job_id: i64) -> BoxFuture<Result<Option<TorrentStatus>, ProviderError>> {
        let service = self.service.clone();
        Box::pin(async move {
            let status = service
                .torrent_status(job_id)
                .await
                .map_err(|e| provider_error(&e))?;
            Ok(status.map(|s| TorrentStatus {
                info_hash: s.info_hash,
                files: s
                    .files
                    .into_iter()
                    .map(|f| TorrentFile {
                        path: f.path,
                        bytes: f.bytes,
                    })
                    .collect(),
                have_bytes: s.have_bytes,
                uploaded_bytes: s.uploaded_bytes,
                download_bps: s.download_bps,
                upload_bps: s.upload_bps,
            }))
        })
    }

    fn categories(&self) -> BoxFuture<Result<Vec<String>, ProviderError>> {
        let service = self.service.clone();
        Box::pin(async move { service.categories().await.map_err(|e| provider_error(&e)) })
    }

    fn create_category(&self, name: String) -> BoxFuture<Result<(), ProviderError>> {
        let service = self.service.clone();
        Box::pin(async move {
            service
                .create_category(&name)
                .await
                .map_err(|e| provider_error(&e))
        })
    }

    fn set_category(
        &self,
        job_id: i64,
        category: Option<String>,
    ) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let service = self.service.clone();
        Box::pin(async move {
            service
                .set_category(job_id, category.as_deref())
                .await
                .map(summary)
                .map_err(|e| provider_error(&e))
        })
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
    if !state.enabled_adapters.contains(&adapter.info().id) {
        return crate::error(StatusCode::NOT_FOUND, "no such route");
    }
    let mount = adapter.mount_path();
    let (parts, body) = request.into_parts();
    let client = crate::auth::client(
        parts
            .extensions
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>(),
    );
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
    let known = match adapter.credential(&request) {
        Credential::NotNeeded => Ok(Some(())),
        Credential::Missing => return into_response(adapter.rejected(&request, true)),
        Credential::ApiKey(key) => state.auth.user_for_api_key(&key).await.map(|u| u.map(drop)),
        Credential::Session(token) => state
            .auth
            .user_for_session(&token)
            .await
            .map(|u| u.map(drop)),
        Credential::Login { username, password } => {
            return match login(&state, client, &username, &password).await {
                Ok(outcome) => into_response(adapter.logged_in(&request, outcome)),
                Err(e) => crate::internal(&e),
            };
        }
    };
    match known {
        Ok(Some(())) => {}
        Ok(None) => return into_response(adapter.rejected(&request, false)),
        Err(e) => return crate::internal(&e),
    }
    let jobs: Arc<dyn Jobs> = Arc::new(state.jobs.clone());
    into_response(adapter.handle(request, jobs).await)
}

/// A protocol's sign-in, under the same rate limit as the native login. The password may be a
/// user's password or one of their API keys, since many clients offer only username and password
/// fields. A key works whatever the username says, because the key alone identifies its owner.
async fn login(
    state: &ApiState,
    client: std::net::IpAddr,
    username: &str,
    password: &str,
) -> Result<LoginOutcome, QueueError> {
    let now = std::time::Instant::now;
    if let Err(wait) = state.limiter.attempt(client, username, now()) {
        // Round up so a client that waits exactly this long is let through.
        let seconds = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
        return Ok(LoginOutcome::RetryLater { seconds });
    }
    let user = match state.auth.user_for_api_key(password).await? {
        Some(user) => Some(user),
        None => match state.auth.login(username, password).await {
            Ok(user) => user,
            Err(QueueError::Busy) => {
                state.limiter.not_checked(client, username, now());
                return Ok(LoginOutcome::RetryLater { seconds: 1 });
            }
            Err(e) => return Err(e),
        },
    };
    let Some(user) = user else {
        return Ok(LoginOutcome::Refused);
    };
    state.limiter.succeeded(client, username, now());
    let lifetime = state.settings.session_lifetime;
    let session = state.auth.start_session(user.id, lifetime).await?;
    Ok(LoginOutcome::Accepted {
        session,
        lifetime_secs: lifetime.as_secs(),
    })
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
