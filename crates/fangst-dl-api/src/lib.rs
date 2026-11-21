//! The native HTTP API of the download daemon.
//!
//! Everything lives under `/api/v1` and needs the `X-Api-Key` header or a session cookie from
//! `/api/v1/auth/login`, except health, login and logout. Health stays open so monitoring and
//! container health checks work without a secret. The OpenAPI document is generated
//! from the handlers; `fangst-dl openapi --check` keeps the committed copy in sync.

use std::sync::Arc;

mod adapters;
mod auth;
mod fetch;
mod limit;

pub use auth::{AuthSettings, LoginRequest, PasswordChange, UserDto};
pub use fetch::FetchLimits;
pub use limit::LoginLimits;

use axum::Json;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use fangst_downloads::{Auth, Change, Error as QueueError, Event, Job, Service, TorrentSource};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

/// NZBs can list hundreds of thousands of segments; fangst-nzb caps them at this size too.
const MAX_NZB_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
struct ApiState {
    service: Service,
    auth: Auth,
    settings: Arc<AuthSettings>,
    limiter: Arc<limit::Limiter>,
    jobs: adapters::ServiceJobs,
}

#[derive(OpenApi)]
#[openapi(
    info(title = "Fangst download daemon", description = "Native API of fangst-dl."),
    paths(health, auth::login, auth::logout, auth::me, auth::change_password, list_jobs, add_job, get_job, pause_job, resume_job, delete_job, job_timeline, job_torrent, set_job_category, list_categories, create_category, get_category_seeding, set_category_seeding, events, test_server),
    components(schemas(Health, JobDto, EventDto, ApiError, ChangeDto, JobStatus, JobKind, TorrentStatusDto, TorrentFileDto, CategoryChange, CategorySeedingDto, AddJobLink, AddJobUrl, AddJobMagnet, ServerTestRequest, ServerTestResult, UserDto, LoginRequest, PasswordChange)),
    modifiers(&ApiKeyScheme),
    security(("api_key" = []), ("session" = []))
)]
struct ApiDoc;

struct ApiKeyScheme;

impl utoipa::Modify for ApiKeyScheme {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-Api-Key"))),
        );
        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(auth::SESSION_COOKIE))),
        );
    }
}

/// The OpenAPI document as pretty JSON with a trailing newline, the form committed to the repo.
///
/// # Panics
/// Never in practice: the document is built from static data.
#[must_use]
pub fn openapi_json() -> String {
    let mut json = ApiDoc::openapi()
        .to_pretty_json()
        .expect("OpenAPI document serializes");
    json.push('\n');
    json
}

/// All routes, with the caller check in front of everything but health, login and logout.
///
/// Serve it with `into_make_service_with_connect_info::<SocketAddr>()`; without the client address
/// every caller shares one login rate limit.
pub fn router(service: Service, auth: Auth, settings: AuthSettings) -> Router {
    router_with_adapters(service, auth, settings, FetchLimits::default(), &[])
}

/// [`router`] plus each enabled API adapter at its mount path. Adapter requests authenticate
/// with the adapter's own credential, checked against the same user API keys.
pub fn router_with_adapters(
    service: Service,
    auth: Auth,
    settings: AuthSettings,
    fetch: FetchLimits,
    adapters: &[Arc<dyn fangst_providers::ApiAdapter>],
) -> Router {
    let state = ApiState {
        jobs: adapters::ServiceJobs {
            service: service.clone(),
            fetcher: fetch::Fetcher::new(fetch),
        },
        service,
        auth,
        limiter: Arc::new(limit::Limiter::new(settings.login)),
        settings: Arc::new(settings),
    };
    let protected = Router::new()
        .route(
            "/api/v1/jobs",
            get(list_jobs)
                .post(add_job)
                .layer(DefaultBodyLimit::max(MAX_NZB_BYTES)),
        )
        .route("/api/v1/jobs/{id}", get(get_job).delete(delete_job))
        .route("/api/v1/jobs/{id}/pause", post(pause_job))
        .route("/api/v1/jobs/{id}/resume", post(resume_job))
        .route("/api/v1/jobs/{id}/timeline", get(job_timeline))
        .route("/api/v1/jobs/{id}/torrent", get(job_torrent))
        .route("/api/v1/jobs/{id}/category", put(set_job_category))
        .route("/api/v1/categories", get(list_categories))
        .route("/api/v1/categories/{name}", put(create_category))
        .route(
            "/api/v1/categories/{name}/seeding",
            get(get_category_seeding).put(set_category_seeding),
        )
        .route("/api/v1/events", get(events))
        .route("/api/v1/servers/test", post(test_server))
        .route("/api/v1/auth/me", get(auth::me))
        .route("/api/v1/auth/password", put(auth::change_password))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_user,
        ));
    let mut app = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/logout", post(auth::logout))
        // Keeps unknown native paths native when an adapter is mounted at `/api`.
        .route("/api/v1/{*rest}", axum::routing::any(not_found))
        .merge(protected);
    for adapter in adapters {
        app = adapters::mount(app, adapter);
    }
    app.with_state(state)
}

async fn not_found() -> Response {
    error(StatusCode::NOT_FOUND, "no such route")
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    pub error: String,
}

pub(crate) fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ApiError {
            error: message.into(),
        }),
    )
        .into_response()
}

fn queue_error(e: &QueueError) -> Response {
    match e {
        QueueError::NotFound(_) => error(StatusCode::NOT_FOUND, e.to_string()),
        QueueError::InvalidNzb(_) | QueueError::InvalidTorrent(_) => {
            error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
        }
        QueueError::InvalidState { .. } | QueueError::DuplicateTorrent(_) => {
            error(StatusCode::CONFLICT, e.to_string())
        }
        QueueError::Invalid(_) => error(StatusCode::BAD_REQUEST, e.to_string()),
        QueueError::Busy => busy(),
        // Says which folder could not be removed and why, which the caller can act on.
        QueueError::Download(_) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        _ => internal(e),
    }
}

/// Database and file errors name paths, tables and library internals; callers, some of them not
/// signed in, get a fixed message and the detail goes to the daemon's log.
pub(crate) fn internal(e: &QueueError) -> Response {
    eprintln!("error: {e}");
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal error; the daemon's log has the details",
    )
}

/// Every password-hashing slot is taken. One second is about how long a check takes.
pub(crate) fn busy() -> Response {
    let mut response = error(
        StatusCode::SERVICE_UNAVAILABLE,
        QueueError::Busy.to_string(),
    );
    response.headers_mut().insert(
        axum::http::header::RETRY_AFTER,
        axum::http::HeaderValue::from_static("1"),
    );
    response
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct Health {
    pub status: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum JobKind {
    Usenet,
    Torrent,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq)]
pub struct JobDto {
    pub id: i64,
    pub kind: JobKind,
    pub name: String,
    pub category: Option<String>,
    pub priority: i64,
    pub status: JobStatus,
    /// Why the job failed, when it did.
    pub error: Option<String>,
    pub output_path: Option<String>,
    pub total_bytes: i64,
    /// Bytes on disk. Live while the job downloads, otherwise as last recorded; a completed job
    /// counts as `total_bytes`.
    pub downloaded_bytes: i64,
    /// Averaged over the last few seconds; 0 unless the job is downloading.
    pub rate_bytes_per_sec: i64,
    /// Seconds left at the current rate; `null` when not downloading or not yet known.
    pub eta_seconds: Option<i64>,
    /// Unix seconds.
    pub added_at: i64,
    pub finished_at: Option<i64>,
    /// A completed torrent job that still uploads to peers; it stops once its category's ratio
    /// or time limit is reached. Always `false` for usenet jobs.
    pub seeding: bool,
    /// Bytes a torrent job sent to peers, every run counted; live while it seeds.
    pub uploaded_bytes: i64,
    /// Uploaded bytes over `total_bytes`; 0 while the size is unknown.
    pub ratio: f64,
    /// Seconds a torrent job seeded after its download finished, every run counted.
    pub seeding_secs: i64,
}

impl From<Job> for JobDto {
    fn from(job: Job) -> Self {
        use fangst_downloads::{Kind, Status};
        let ratio = if job.total_bytes > 0 {
            #[allow(clippy::cast_precision_loss)] // A ratio; 2^52 bytes is far away.
            let ratio = job.uploaded_bytes as f64 / job.total_bytes as f64;
            ratio
        } else {
            0.0
        };
        Self {
            id: job.id,
            kind: match job.kind {
                Kind::Usenet => JobKind::Usenet,
                Kind::Torrent => JobKind::Torrent,
            },
            name: job.name,
            category: job.category,
            priority: job.priority,
            status: match job.status {
                Status::Queued => JobStatus::Queued,
                Status::Downloading => JobStatus::Downloading,
                Status::Paused => JobStatus::Paused,
                Status::Completed => JobStatus::Completed,
                Status::Failed => JobStatus::Failed,
            },
            error: job.error,
            output_path: job.output_path.map(|p| p.to_string_lossy().into_owned()),
            total_bytes: job.total_bytes,
            downloaded_bytes: job.downloaded_bytes,
            rate_bytes_per_sec: job.rate_bytes_per_sec,
            eta_seconds: job.eta_seconds,
            added_at: job.added_at,
            finished_at: job.finished_at,
            seeding: job.seeding,
            uploaded_bytes: job.uploaded_bytes,
            ratio,
            seeding_secs: job.seeding_secs,
        }
    }
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct EventDto {
    /// Unix seconds.
    pub at: i64,
    pub kind: String,
    pub message: String,
}

impl From<Event> for EventDto {
    fn from(event: Event) -> Self {
        Self {
            at: event.at,
            kind: event.kind,
            message: event.message,
        }
    }
}

/// A message on the event stream.
#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ChangeDto {
    Updated {
        job: JobDto,
    },
    Deleted {
        id: i64,
    },
    /// Sent about once a second for each downloading job.
    Progress {
        id: i64,
        downloaded_bytes: i64,
        rate_bytes_per_sec: i64,
        /// `null` while the rate or the size is unknown.
        eta_seconds: Option<i64>,
    },
}

fn progress_message(id: i64, progress: fangst_downloads::Progress) -> ChangeDto {
    let signed = |n: u64| i64::try_from(n).unwrap_or(i64::MAX);
    ChangeDto::Progress {
        id,
        downloaded_bytes: signed(progress.downloaded_bytes),
        rate_bytes_per_sec: signed(progress.rate_bytes_per_sec),
        eta_seconds: progress.eta_seconds.map(signed),
    }
}

#[utoipa::path(get, path = "/api/v1/health", security(()), responses((status = 200, body = Health)))]
async fn health() -> Json<Health> {
    Json(Health {
        status: "ok".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

#[utoipa::path(get, path = "/api/v1/jobs", responses((status = 200, body = [JobDto]), (status = 401, body = ApiError)))]
async fn list_jobs(State(state): State<ApiState>) -> Response {
    match state.service.store().list().await {
        Ok(jobs) => Json(jobs.into_iter().map(JobDto::from).collect::<Vec<_>>()).into_response(),
        Err(e) => queue_error(&e),
    }
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct AddJobQuery {
    /// Shown in the queue; defaults to the NZB's title or the torrent's name.
    name: Option<String>,
    category: Option<String>,
    /// Higher runs first.
    priority: Option<i64>,
    /// A magnet link to add instead of a request body.
    magnet: Option<String>,
}

/// An NZB or `.torrent` for the daemon to download itself.
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddJobUrl {
    /// `http` or `https`; redirects are followed only to `http` and `https`, except that a
    /// redirect to a magnet link adds that link. A `magnet:` link here is added as one.
    pub url: String,
}

/// A torrent by magnet link. Only links with a v1 (`btih`) info hash are accepted.
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddJobMagnet {
    pub magnet: String,
}

/// The JSON forms of `POST /api/v1/jobs`: exactly one of `url` or `magnet`.
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(untagged)]
pub enum AddJobLink {
    Url(AddJobUrl),
    Magnet(AddJobMagnet),
}

/// What a request to add a job carries, once URLs are fetched.
enum Input {
    Nzb {
        bytes: Vec<u8>,
        file_name: Option<String>,
    },
    Torrent {
        bytes: Vec<u8>,
        file_name: Option<String>,
    },
    Magnet(String),
}

const TORRENT_TYPE: &str = "application/x-bittorrent";

#[utoipa::path(
    post,
    path = "/api/v1/jobs",
    params(AddJobQuery),
    request_body(
        description = "An NZB file, a `.torrent` file, or JSON naming a URL to fetch either from or a magnet link. With `?magnet=` the body is ignored. Without `name` the job is named after the NZB's title or the torrent's name, then the file name in the URL.",
        content(
            (AddJobLink = "application/json"),
            (Vec<u8> = "application/x-nzb"),
            (Vec<u8> = "application/x-bittorrent")
        )
    ),
    responses(
        (status = 201, body = JobDto),
        (status = 400, body = ApiError, description = "The JSON is invalid, or the URL is invalid or not http(s)"),
        (status = 401, body = ApiError),
        (status = 409, body = ApiError, description = "The same torrent is already a job"),
        (status = 422, body = ApiError, description = "Not a valid NZB, torrent or magnet link, or the fetched file is too large"),
        (status = 502, body = ApiError, description = "The URL could not be fetched")
    )
)]
async fn add_job(
    State(state): State<ApiState>,
    Query(query): Query<AddJobQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .trim_start();
    let input = if let Some(link) = query.magnet {
        Input::Magnet(link)
    } else if content_type.starts_with("application/json") {
        match link_input(&state, &body).await {
            Ok(input) => input,
            Err(response) => return *response,
        }
    } else if content_type.starts_with(TORRENT_TYPE) {
        Input::Torrent {
            bytes: body.to_vec(),
            file_name: None,
        }
    } else {
        Input::Nzb {
            bytes: body.to_vec(),
            file_name: None,
        }
    };
    let category = query.category.as_deref();
    let priority = query.priority.unwrap_or(0);
    let added = match input {
        Input::Nzb { bytes, file_name } => {
            let name = query
                .name
                .or_else(|| fangst_nzb_title(&bytes))
                .or(file_name)
                .unwrap_or_else(|| UNTITLED.to_owned());
            state
                .service
                .add_nzb(&name, category, priority, &bytes)
                .await
        }
        Input::Torrent { bytes, file_name } => {
            add_torrent(
                &state.service,
                query.name,
                file_name,
                category,
                priority,
                TorrentSource::File(&bytes),
            )
            .await
        }
        Input::Magnet(link) => {
            add_torrent(
                &state.service,
                query.name,
                None,
                category,
                priority,
                TorrentSource::Magnet(&link),
            )
            .await
        }
    };
    match added {
        Ok(id) => match state.service.store().get(id).await {
            Ok(job) => (StatusCode::CREATED, Json(JobDto::from(job))).into_response(),
            Err(e) => queue_error(&e),
        },
        Err(e) => queue_error(&e),
    }
}

const UNTITLED: &str = "Untitled";

/// Queues a torrent named `name`, else after the torrent or the magnet link's `dn`, else after
/// `file_name`, the file in the URL it came from.
pub(crate) async fn add_torrent(
    service: &Service,
    name: Option<String>,
    file_name: Option<String>,
    category: Option<&str>,
    priority: i64,
    source: TorrentSource<'_>,
) -> Result<i64, QueueError> {
    let from_source = match source {
        TorrentSource::File(bytes) => fangst_torrent::inspect(bytes).ok().and_then(|m| m.name),
        TorrentSource::Magnet(link) => fangst_torrent::parse_magnet(link).ok().and_then(|m| m.name),
    };
    let name = name
        .filter(|n| !n.trim().is_empty())
        .or(from_source)
        .or(file_name)
        .unwrap_or_else(|| UNTITLED.to_owned());
    service.add_torrent(&name, category, priority, source).await
}

async fn link_input(state: &ApiState, body: &[u8]) -> Result<Input, Box<Response>> {
    let request: AddJobLink = serde_json::from_slice(body).map_err(|_| {
        Box::new(error(
            StatusCode::BAD_REQUEST,
            "the JSON body must be {\"url\": \"...\"} or {\"magnet\": \"...\"}",
        ))
    })?;
    let url = match request {
        AddJobLink::Magnet(AddJobMagnet { magnet }) => return Ok(Input::Magnet(magnet)),
        AddJobLink::Url(AddJobUrl { url }) if url.starts_with("magnet:") => {
            return Ok(Input::Magnet(url));
        }
        AddJobLink::Url(AddJobUrl { url }) => url,
    };
    let fetched = state.jobs.fetcher.fetch(&url).await.map_err(|e| {
        let status = match e {
            fetch::FetchError::InvalidUrl(_) => StatusCode::BAD_REQUEST,
            fetch::FetchError::TooLarge(_) => StatusCode::UNPROCESSABLE_ENTITY,
            fetch::FetchError::Status(_) | fetch::FetchError::Network(_) => StatusCode::BAD_GATEWAY,
        };
        Box::new(error(status, e.to_string()))
    })?;
    Ok(if let Some(link) = fetched.magnet {
        Input::Magnet(link)
    } else if fetched.is_torrent() {
        Input::Torrent {
            bytes: fetched.bytes,
            file_name: fetched.file_name,
        }
    } else {
        Input::Nzb {
            bytes: fetched.bytes,
            file_name: fetched.file_name,
        }
    })
}

pub(crate) fn fangst_nzb_title(body: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    let start = text.find("type=\"title\">")? + "type=\"title\">".len();
    let end = text[start..].find("</meta>")?;
    let title = text[start..start + end].trim();
    (!title.is_empty()).then(|| title.to_owned())
}

#[utoipa::path(
    get,
    path = "/api/v1/jobs/{id}",
    params(("id" = i64, Path)),
    responses((status = 200, body = JobDto), (status = 404, body = ApiError))
)]
async fn get_job(State(state): State<ApiState>, Path(id): Path<i64>) -> Response {
    match state.service.store().get(id).await {
        Ok(job) => Json(JobDto::from(job)).into_response(),
        Err(e) => queue_error(&e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/jobs/{id}/pause",
    params(("id" = i64, Path)),
    responses((status = 200, body = JobDto), (status = 404, body = ApiError), (status = 409, body = ApiError))
)]
async fn pause_job(State(state): State<ApiState>, Path(id): Path<i64>) -> Response {
    match state.service.pause(id).await {
        Ok(job) => Json(JobDto::from(job)).into_response(),
        Err(e) => queue_error(&e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/jobs/{id}/resume",
    params(("id" = i64, Path)),
    responses((status = 200, body = JobDto), (status = 404, body = ApiError), (status = 409, body = ApiError))
)]
async fn resume_job(State(state): State<ApiState>, Path(id): Path<i64>) -> Response {
    match state.service.resume(id).await {
        Ok(job) => Json(JobDto::from(job)).into_response(),
        Err(e) => queue_error(&e),
    }
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct DeleteQuery {
    /// Also remove the job's folder and everything downloaded into it.
    delete_files: Option<bool>,
}

#[utoipa::path(
    delete,
    path = "/api/v1/jobs/{id}",
    params(("id" = i64, Path), DeleteQuery),
    responses((status = 204), (status = 404, body = ApiError))
)]
async fn delete_job(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
    Query(query): Query<DeleteQuery>,
) -> Response {
    match state
        .service
        .delete(id, query.delete_files.unwrap_or(false))
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => queue_error(&e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/jobs/{id}/timeline",
    params(("id" = i64, Path)),
    responses((status = 200, body = [EventDto]), (status = 404, body = ApiError))
)]
async fn job_timeline(State(state): State<ApiState>, Path(id): Path<i64>) -> Response {
    if let Err(e) = state.service.store().get(id).await {
        return queue_error(&e);
    }
    match state.service.store().timeline(id).await {
        Ok(events) => {
            Json(events.into_iter().map(EventDto::from).collect::<Vec<_>>()).into_response()
        }
        Err(e) => queue_error(&e),
    }
}

/// A torrent job's hash, files and live progress.
#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq)]
pub struct TorrentStatusDto {
    /// Lowercase hex of the v1 info hash.
    pub info_hash: String,
    /// Empty until the metadata is known, which for a magnet link means until a peer sends it.
    pub files: Vec<TorrentFileDto>,
    /// Bytes on disk that passed the hash check.
    pub have_bytes: u64,
    /// Bytes sent to peers over the job's life, every run counted.
    pub uploaded_bytes: u64,
    /// Bytes per second; 0 while not running.
    pub download_bps: u64,
    pub upload_bps: u64,
    /// The job completed and still uploads to peers.
    pub seeding: bool,
    /// Seconds seeded after the download finished, every run counted.
    pub seeding_secs: u64,
    /// The ratio at which seeding stops, from the job's category or the defaults; `null` when off.
    pub ratio_limit: Option<f64>,
    /// Seconds of seeding after which it stops; `null` when off.
    pub seeding_time_limit_secs: Option<u64>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct TorrentFileDto {
    /// Relative to the job folder, with `/` between folders.
    pub path: String,
    pub bytes: u64,
}

impl From<fangst_downloads::TorrentStatus> for TorrentStatusDto {
    fn from(status: fangst_downloads::TorrentStatus) -> Self {
        Self {
            info_hash: status.info_hash,
            files: status
                .files
                .into_iter()
                .map(|f| TorrentFileDto {
                    path: f.path,
                    bytes: f.bytes,
                })
                .collect(),
            have_bytes: status.have_bytes,
            uploaded_bytes: status.uploaded_bytes,
            download_bps: status.download_bps,
            upload_bps: status.upload_bps,
            seeding: status.seeding,
            seeding_secs: status.seeding_secs,
            ratio_limit: status.ratio_limit,
            seeding_time_limit_secs: status.seeding_time_limit_secs,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/jobs/{id}/torrent",
    params(("id" = i64, Path)),
    responses((status = 200, body = TorrentStatusDto), (status = 404, body = ApiError, description = "No such job, or not a torrent job"))
)]
async fn job_torrent(State(state): State<ApiState>, Path(id): Path<i64>) -> Response {
    match state.service.torrent_status(id).await {
        Ok(Some(status)) => Json(TorrentStatusDto::from(status)).into_response(),
        Ok(None) => error(
            StatusCode::NOT_FOUND,
            format!("job {id} is not a torrent job"),
        ),
        Err(e) => queue_error(&e),
    }
}

/// The category to move a job to; `null` for none.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct CategoryChange {
    pub category: Option<String>,
}

#[utoipa::path(
    put,
    path = "/api/v1/jobs/{id}/category",
    params(("id" = i64, Path)),
    request_body = CategoryChange,
    responses((status = 200, body = JobDto), (status = 400, body = ApiError), (status = 404, body = ApiError))
)]
async fn set_job_category(
    State(state): State<ApiState>,
    Path(id): Path<i64>,
    Json(body): Json<CategoryChange>,
) -> Response {
    match state
        .service
        .set_category(id, body.category.as_deref())
        .await
    {
        Ok(job) => Json(JobDto::from(job)).into_response(),
        Err(e) => queue_error(&e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/categories",
    description = "Every category: those created ahead of any job and those jobs carry, sorted.",
    responses((status = 200, body = [String]), (status = 401, body = ApiError))
)]
async fn list_categories(State(state): State<ApiState>) -> Response {
    match state.service.categories().await {
        Ok(categories) => Json(categories).into_response(),
        Err(e) => queue_error(&e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/categories/{name}",
    params(("name" = String, Path)),
    description = "Creates a category; one that exists already is left as it is.",
    responses((status = 204), (status = 400, body = ApiError), (status = 401, body = ApiError))
)]
async fn create_category(State(state): State<ApiState>, Path(name): Path<String>) -> Response {
    match state.service.create_category(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => queue_error(&e),
    }
}

/// A category's own seeding limits. `null` uses the daemon's default; a negative value turns that
/// limit off. A finished torrent stops seeding at whichever limit it reaches first.
#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, Copy, PartialEq)]
pub struct CategorySeedingDto {
    /// Uploaded bytes over the torrent's size.
    pub ratio_limit: Option<f64>,
    /// Seconds of seeding after the download finished.
    pub time_limit_secs: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/categories/{name}/seeding",
    params(("name" = String, Path)),
    description = "A category's own seeding limits; both `null` for a category that sets none.",
    responses((status = 200, body = CategorySeedingDto), (status = 401, body = ApiError))
)]
async fn get_category_seeding(State(state): State<ApiState>, Path(name): Path<String>) -> Response {
    match state.service.category_seeding(&name).await {
        Ok(seeding) => Json(CategorySeedingDto {
            ratio_limit: seeding.ratio_limit,
            time_limit_secs: seeding.time_limit_secs,
        })
        .into_response(),
        Err(e) => queue_error(&e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/categories/{name}/seeding",
    params(("name" = String, Path)),
    request_body = CategorySeedingDto,
    description = "Sets a category's seeding limits, creating the category if needed. Jobs already seeding apply them the next time their counts are written.",
    responses((status = 204), (status = 400, body = ApiError), (status = 401, body = ApiError))
)]
async fn set_category_seeding(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    Json(body): Json<CategorySeedingDto>,
) -> Response {
    let seeding = fangst_downloads::CategorySeeding {
        ratio_limit: body.ratio_limit,
        time_limit_secs: body.time_limit_secs,
    };
    match state.service.set_category_seeding(&name, seeding).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => queue_error(&e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/events",
    description = "WebSocket. Each message is a `ChangeDto` as JSON. Browsers pass the key as `?api_key=`.",
    responses((status = 101, description = "Switching to WebSocket", body = ChangeDto), (status = 401, body = ApiError))
)]
async fn events(
    State(state): State<ApiState>,
    ws: WebSocketUpgrade,
    _headers: HeaderMap,
) -> Response {
    // Subscribe before answering the handshake: the upgrade callback runs after the client already
    // sees the socket as open, so a change made in between would otherwise never be sent.
    let changes = state.service.subscribe();
    ws.on_upgrade(move |socket| stream_changes(socket, state, changes))
}

async fn stream_changes(
    mut socket: WebSocket,
    state: ApiState,
    mut changes: tokio::sync::broadcast::Receiver<Change>,
) {
    loop {
        tokio::select! {
            change = changes.recv() => {
                let message = match change {
                    Ok(Change::Updated(id)) => match state.service.store().get(id).await {
                        Ok(job) => ChangeDto::Updated { job: job.into() },
                        Err(_) => continue,
                    },
                    Ok(Change::Deleted(id)) => ChangeDto::Deleted { id },
                    Ok(Change::Progress(id, progress)) => progress_message(id, progress),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                };
                let Ok(text) = serde_json::to_string(&message) else { continue };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => {
                if matches!(incoming, None | Some(Err(_) | Ok(Message::Close(_)))) {
                    return;
                }
            }
        }
    }
}

/// A usenet server to try. Nothing is saved.
#[derive(Deserialize, Serialize, ToSchema)]
pub struct ServerTestRequest {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_true")]
    pub tls: bool,
    pub username: Option<String>,
    pub password: Option<String>,
    /// How many connections to open at once; use the count you plan to configure.
    #[serde(default = "default_one")]
    pub connections: usize,
}

fn default_true() -> bool {
    true
}

fn default_one() -> usize {
    1
}

#[derive(Deserialize, Serialize, ToSchema, Debug, PartialEq, Eq)]
pub struct ServerTestResult {
    pub ok: bool,
    /// Stable name of what went wrong: `dns`, `refused`, `timeout`, `tls`, `network`,
    /// `auth_failed`, `auth_required`, `unavailable`, `protocol` or `connection_limit`.
    pub cause: Option<String>,
    /// What happened, in words a user can act on. Never contains the password.
    pub message: String,
    pub connections_accepted: usize,
    pub connections_requested: usize,
}

/// Opening a connection includes DNS, TCP and the TLS handshake; a healthy provider answers well within this.
const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Upper bound on connections a test may open, so the endpoint can't be used to flood a server.
const MAX_TEST_CONNECTIONS: usize = 100;

#[utoipa::path(
    post,
    path = "/api/v1/servers/test",
    request_body = ServerTestRequest,
    description = "Connects with every requested connection at once and logs in. A failed test is still a 200; read `ok` and `cause`.",
    responses((status = 200, body = ServerTestResult), (status = 400, body = ApiError), (status = 401, body = ApiError))
)]
async fn test_server(Json(request): Json<ServerTestRequest>) -> Response {
    if request.connections == 0 || request.connections > MAX_TEST_CONNECTIONS {
        return error(
            StatusCode::BAD_REQUEST,
            format!("connections must be between 1 and {MAX_TEST_CONNECTIONS}"),
        );
    }
    let tls = if request.tls {
        match fangst_nntp::native_tls_config() {
            Ok(tls) => Some(tls),
            Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    } else {
        None
    };
    let credentials = request
        .username
        .map(|user| (user, request.password.unwrap_or_default()));
    let config = fangst_nntp::ServerConfig {
        host: request.host,
        port: request.port,
        tls,
        credentials,
        max_connections: request.connections,
        connect_timeout: TEST_TIMEOUT,
        command_timeout: TEST_TIMEOUT,
    };
    let result = match fangst_nntp::check_server(&config).await {
        Ok(check) => ServerTestResult {
            ok: true,
            cause: None,
            message: format!(
                "connected and logged in with {} connections",
                check.connections_accepted
            ),
            connections_accepted: check.connections_accepted,
            connections_requested: check.connections_requested,
        },
        Err(e) => ServerTestResult {
            ok: false,
            cause: Some(e.cause().to_owned()),
            message: e.to_string(),
            connections_accepted: match e {
                fangst_nntp::CheckError::ConnectionLimit { accepted, .. } => accepted,
                fangst_nntp::CheckError::Connect(_) => 0,
            },
            connections_requested: config.max_connections,
        },
    };
    Json(result).into_response()
}
