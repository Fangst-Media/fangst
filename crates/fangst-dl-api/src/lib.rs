//! The native HTTP API of the download daemon.
//!
//! Everything lives under `/api/v1` and needs the `X-Api-Key` header, except `/api/v1/health` so
//! monitoring and container health checks work without a secret. The OpenAPI document is generated
//! from the handlers; `fangst-dl openapi --check` keeps the committed copy in sync.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use fangst_downloads::{Change, Error as QueueError, Event, Job, Service};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

/// NZBs can list hundreds of thousands of segments; fangst-nzb caps them at this size too.
const MAX_NZB_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
struct ApiState {
    service: Service,
    api_key: Arc<str>,
}

#[derive(OpenApi)]
#[openapi(
    info(title = "Fangst download daemon", description = "Native API of fangst-dl."),
    paths(health, list_jobs, add_job, get_job, pause_job, resume_job, delete_job, job_timeline, events, test_server),
    components(schemas(Health, JobDto, EventDto, ApiError, ChangeDto, JobStatus, JobKind, ServerTestRequest, ServerTestResult)),
    modifiers(&ApiKeyScheme),
    security(("api_key" = []))
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

/// All routes, with the API key check in front of everything but health.
pub fn router(service: Service, api_key: &str) -> Router {
    let state = ApiState {
        service,
        api_key: Arc::from(api_key),
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
        .route("/api/v1/events", get(events))
        .route("/api/v1/servers/test", post(test_server))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ));
    Router::new()
        .route("/api/v1/health", get(health))
        .merge(protected)
        .with_state(state)
}

async fn require_api_key(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    let header = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok());
    // Browsers can't set headers on a WebSocket handshake, so the event stream also takes a query key.
    let query = (request.uri().path() == "/api/v1/events")
        .then(|| request.uri().query())
        .flatten()
        .and_then(|q| q.split('&').find_map(|pair| pair.strip_prefix("api_key=")));
    match header.or(query) {
        Some(key) if constant_time_eq(key.as_bytes(), state.api_key.as_bytes()) => {
            next.run(request).await
        }
        _ => error(
            StatusCode::UNAUTHORIZED,
            "missing or wrong X-Api-Key header",
        ),
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    pub error: String,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
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
        QueueError::InvalidNzb(_) => error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
        QueueError::InvalidState { .. } => error(StatusCode::CONFLICT, e.to_string()),
        _ => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
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

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
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
    /// Unix seconds.
    pub added_at: i64,
    pub finished_at: Option<i64>,
}

impl From<Job> for JobDto {
    fn from(job: Job) -> Self {
        use fangst_downloads::{Kind, Status};
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
            added_at: job.added_at,
            finished_at: job.finished_at,
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
#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ChangeDto {
    Updated { job: JobDto },
    Deleted { id: i64 },
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
struct AddJobQuery {
    /// Shown in the queue; defaults to the NZB's title.
    name: Option<String>,
    category: Option<String>,
    /// Higher runs first.
    priority: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/v1/jobs",
    params(AddJobQuery),
    request_body(content = Vec<u8>, content_type = "application/x-nzb", description = "The NZB file"),
    responses((status = 201, body = JobDto), (status = 422, body = ApiError), (status = 401, body = ApiError))
)]
async fn add_job(
    State(state): State<ApiState>,
    Query(query): Query<AddJobQuery>,
    body: axum::body::Bytes,
) -> Response {
    let name = query
        .name
        .clone()
        .unwrap_or_else(|| fangst_nzb_title(&body).unwrap_or_else(|| "Untitled".to_owned()));
    let added = state
        .service
        .add_nzb(
            &name,
            query.category.as_deref(),
            query.priority.unwrap_or(0),
            &body,
        )
        .await;
    match added {
        Ok(id) => match state.service.store().get(id).await {
            Ok(job) => (StatusCode::CREATED, Json(JobDto::from(job))).into_response(),
            Err(e) => queue_error(&e),
        },
        Err(e) => queue_error(&e),
    }
}

fn fangst_nzb_title(body: &[u8]) -> Option<String> {
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
    ws.on_upgrade(move |socket| stream_changes(socket, state))
}

async fn stream_changes(mut socket: WebSocket, state: ApiState) {
    let mut changes = state.service.subscribe();
    loop {
        tokio::select! {
            change = changes.recv() => {
                let message = match change {
                    Ok(Change::Updated(id)) => match state.service.store().get(id).await {
                        Ok(job) => ChangeDto::Updated { job: job.into() },
                        Err(_) => continue,
                    },
                    Ok(Change::Deleted(id)) => ChangeDto::Deleted { id },
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
