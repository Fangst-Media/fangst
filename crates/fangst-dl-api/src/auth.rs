//! Who is calling: API keys, session cookies, and the login, logout and password routes.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Extension, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use fangst_downloads::{Error as QueueError, User};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::limit::LoginLimits;
use crate::{ApiError, ApiState, busy, error, internal};

pub(crate) const SESSION_COOKIE: &str = "fangst_session";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthSettings {
    pub session_lifetime: Duration,
    /// Mark the session cookie `Secure`. Turn on when the daemon is only reached over HTTPS;
    /// browsers then refuse to send the cookie over plain HTTP.
    pub secure_cookie: bool,
    pub login: LoginLimits,
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            // A month keeps a home server's browser signed in without asking every week.
            session_lifetime: Duration::from_hours(30 * 24),
            secure_cookie: false,
            login: LoginLimits::default(),
        }
    }
}

/// The signed-in user, put on the request by [`require_user`].
#[derive(Clone)]
pub(crate) struct Caller {
    user: User,
    session: Option<String>,
}

pub(crate) async fn require_user(
    State(state): State<ApiState>,
    mut request: Request,
    next: Next,
) -> Response {
    let credential = credential(&request);
    // SameSite=Strict stops other sites, but not other origins on the same site: another port on
    // this host, or a sibling subdomain such as another app behind the same domain. Those can
    // send the cookie, so a cookie may only change state, or open the event stream, when the
    // request comes from this origin. API keys are not sent automatically and need no check.
    let cross_origin = matches!(credential, Some(Credential::Session(_)))
        && needs_same_origin(request.method(), request.uri())
        && !same_origin(request.headers(), request.uri());
    match identify(&state, credential).await {
        Ok(Some(_)) if cross_origin => error(
            StatusCode::FORBIDDEN,
            "a session cookie only works from the daemon's own pages; other callers need an API key",
        ),
        Ok(Some(caller)) => {
            request.extensions_mut().insert(caller);
            next.run(request).await
        }
        Ok(None) => error(
            StatusCode::UNAUTHORIZED,
            "missing or wrong X-Api-Key header or session",
        ),
        Err(e) => internal(&e),
    }
}

enum Credential {
    ApiKey(String),
    Session(String),
}

fn credential(request: &Request) -> Option<Credential> {
    let header = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    // Browsers can't set headers on a WebSocket handshake, so the event stream also takes a query
    // key. It arrives URL-encoded; keys with `+`, `/` or `=` only match after decoding.
    let query = (request.uri().path() == "/api/v1/events")
        .then(|| {
            Query::<HashMap<String, String>>::try_from_uri(request.uri())
                .ok()
                .and_then(|Query(mut params)| params.remove("api_key"))
        })
        .flatten();
    header
        .or(query)
        .map(Credential::ApiKey)
        .or_else(|| session_cookie(request.headers()).map(Credential::Session))
}

async fn identify(
    state: &ApiState,
    credential: Option<Credential>,
) -> Result<Option<Caller>, QueueError> {
    match credential {
        Some(Credential::ApiKey(key)) => {
            Ok(state.auth.user_for_api_key(&key).await?.map(|user| Caller {
                user,
                session: None,
            }))
        }
        Some(Credential::Session(token)) => {
            Ok(state
                .auth
                .user_for_session(&token)
                .await?
                .map(|user| Caller {
                    user,
                    session: Some(token),
                }))
        }
        None => Ok(None),
    }
}

fn needs_same_origin(method: &Method, uri: &Uri) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
        || uri.path() == "/api/v1/events"
}

/// True when the browser says the request comes from this origin: `Sec-Fetch-Site: same-origin`,
/// or an `Origin` whose host and port are the ones the request was sent to. A proxy that rewrites
/// `Host` still passes through the first.
fn same_origin(headers: &HeaderMap, uri: &Uri) -> bool {
    let text = |name| {
        headers
            .get(name)
            .and_then(|v: &HeaderValue| v.to_str().ok())
    };
    if text("sec-fetch-site") == Some("same-origin") {
        return true;
    }
    let Some(origin) = text(header::ORIGIN.as_str()) else {
        return false;
    };
    let Some(origin_authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let target = uri
        .authority()
        .map(axum::http::uri::Authority::as_str)
        .or_else(|| text(header::HOST.as_str()));
    target.is_some_and(|target| target.eq_ignore_ascii_case(origin_authority))
}

fn session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == SESSION_COOKIE && !value.is_empty()).then(|| value.to_owned())
        })
}

fn set_cookie(settings: &AuthSettings, token: &str, max_age: Duration) -> HeaderValue {
    let secure = if settings.secure_cookie {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{secure}",
        max_age.as_secs()
    ))
    .expect("cookie is ASCII")
}

pub(crate) fn client(connect: Option<&ConnectInfo<SocketAddr>>) -> IpAddr {
    // Without connection info (in-process tests) every caller shares one bucket.
    connect.map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |c| c.0.ip())
}

fn too_many(wait: Duration) -> Response {
    // Round up so a client that waits exactly this long is let through.
    let seconds = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
    let mut response = error(
        StatusCode::TOO_MANY_REQUESTS,
        format!("too many failed attempts; try again in {seconds} seconds"),
    );
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
    response
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct UserDto {
    pub id: i64,
    pub username: String,
    /// False for a user who can only sign in with an API key so far.
    pub has_password: bool,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct PasswordChange {
    /// Required once the user has a password.
    pub current_password: Option<String>,
    pub new_password: String,
}

const WRONG_LOGIN: &str = "wrong username or password";

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    security(()),
    request_body = LoginRequest,
    description = "Sets an HttpOnly session cookie. Repeated failures from one address are slowed down with 429 and `Retry-After`.",
    responses(
        (status = 200, body = UserDto),
        (status = 401, body = ApiError),
        (status = 429, body = ApiError),
        (status = 503, body = ApiError, description = "Too many password checks at once; retry after `Retry-After`")
    )
)]
pub(crate) async fn login(
    State(state): State<ApiState>,
    connect: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(body): Json<LoginRequest>,
) -> Response {
    let client = client(connect.as_ref().map(|Extension(c)| c));
    if let Err(wait) = state
        .limiter
        .attempt(client, &body.username, Instant::now())
    {
        return too_many(wait);
    }
    let user = match state.auth.login(&body.username, &body.password).await {
        Ok(Some(user)) => user,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, WRONG_LOGIN),
        Err(QueueError::Busy) => {
            state
                .limiter
                .not_checked(client, &body.username, Instant::now());
            return busy();
        }
        Err(e) => return internal(&e),
    };
    state
        .limiter
        .succeeded(client, &body.username, Instant::now());
    let token = match state
        .auth
        .start_session(user.id, state.settings.session_lifetime)
        .await
    {
        Ok(token) => token,
        Err(e) => return internal(&e),
    };
    let mut response = Json(UserDto {
        id: user.id,
        username: user.username,
        has_password: true,
    })
    .into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        set_cookie(&state.settings, &token, state.settings.session_lifetime),
    );
    response
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    security(()),
    description = "Ends the session in the cookie, if any, and clears the cookie.",
    responses((status = 204))
)]
pub(crate) async fn logout(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_cookie(&headers)
        && let Err(e) = state.auth.end_session(&token).await
    {
        return internal(&e);
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        set_cookie(&state.settings, "", Duration::ZERO),
    );
    response
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    responses((status = 200, body = UserDto), (status = 401, body = ApiError))
)]
pub(crate) async fn me(
    State(state): State<ApiState>,
    Extension(caller): Extension<Caller>,
) -> Response {
    match state.auth.has_password(caller.user.id).await {
        Ok(has_password) => Json(UserDto {
            id: caller.user.id,
            username: caller.user.username,
            has_password,
        })
        .into_response(),
        Err(e) => internal(&e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/auth/password",
    request_body = PasswordChange,
    description = "Sets the caller's password and ends their other sessions. A caller signed in with a session gets a fresh session cookie.",
    responses(
        (status = 204),
        (status = 400, body = ApiError),
        (status = 401, body = ApiError),
        (status = 403, body = ApiError, description = "The current password is wrong"),
        (status = 429, body = ApiError),
        (status = 503, body = ApiError)
    )
)]
pub(crate) async fn change_password(
    State(state): State<ApiState>,
    Extension(caller): Extension<Caller>,
    connect: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(body): Json<PasswordChange>,
) -> Response {
    let user_id = caller.user.id;
    let has_password = match state.auth.has_password(user_id).await {
        Ok(has) => has,
        Err(e) => return internal(&e),
    };
    if has_password {
        // A stolen session or key must not be enough to guess the password and take the account.
        let client = client(connect.as_ref().map(|Extension(c)| c));
        let account = caller.user.username.as_str();
        if let Err(wait) = state.limiter.attempt(client, account, Instant::now()) {
            return too_many(wait);
        }
        let current = body.current_password.as_deref().unwrap_or_default();
        match state.auth.check_password(user_id, current).await {
            Ok(true) => state.limiter.succeeded(client, account, Instant::now()),
            Ok(false) => return error(StatusCode::FORBIDDEN, "the current password is wrong"),
            Err(QueueError::Busy) => {
                state.limiter.not_checked(client, account, Instant::now());
                return busy();
            }
            Err(e) => return internal(&e),
        }
    }
    match state.auth.set_password(user_id, &body.new_password).await {
        Ok(()) => {}
        Err(QueueError::Invalid(why)) => return error(StatusCode::BAD_REQUEST, why),
        Err(QueueError::Busy) => return busy(),
        Err(e) => return internal(&e),
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    if caller.session.is_some() {
        match state
            .auth
            .start_session(user_id, state.settings.session_lifetime)
            .await
        {
            Ok(token) => {
                response.headers_mut().insert(
                    header::SET_COOKIE,
                    set_cookie(&state.settings, &token, state.settings.session_lifetime),
                );
            }
            Err(e) => return internal(&e),
        }
    }
    response
}
