//! First-run setup: whether it is done, claiming the owner account with the setup token, and
//! the folder check the setup and settings pages share.
//!
//! A fresh daemon has an owner without a password. Whoever reaches the API first could claim it,
//! so claiming needs the setup token the daemon prints at start, or an API key of the owner, which
//! only the operator has.

use std::net::SocketAddr;
use std::time::Instant;

use axum::Json;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Extension, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use fangst_downloads::Error as QueueError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::{Caller, client, set_cookie, too_many};
use crate::settings::{control_error, owner_only};
use crate::{ApiError, ApiState, UserDto, busy, error, internal};

/// Rate-limit bucket for setup token guesses; shares the login limit's rules.
const SETUP_ACCOUNT: &str = "setup";

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct SetupStatus {
    /// False until the owner chose a password; the setup starts by claiming the account.
    pub owner_has_password: bool,
    /// The setup finished, or the environment configured the daemon.
    pub completed: bool,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct SetupOwner {
    /// The setup token from the daemon's output at start, or the owner's API key.
    pub token: String,
    /// At least 12 characters.
    pub password: String,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct FolderCheckRequest {
    /// Relative paths are inside the data folder.
    pub downloads: String,
    pub media: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct FolderStatus {
    /// The folder as the daemon sees it, absolute.
    pub path: String,
    /// It did not exist and the check created it.
    pub created: bool,
    pub writable: bool,
    /// Why the folder can't be used, when it can't.
    pub problem: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct FolderCheckResult {
    pub downloads: FolderStatus,
    pub media: Option<FolderStatus>,
    /// Whether finished downloads can be hardlinked into the media folder: instant and taking no
    /// extra space. `null` without a usable media folder.
    pub hardlinks: Option<bool>,
    /// Why hardlinks don't work, when they don't.
    pub hardlink_problem: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/setup",
    security(()),
    description = "Whether the first-run setup is still to do. Needs no credentials.",
    responses((status = 200, body = SetupStatus))
)]
pub(crate) async fn status(State(state): State<ApiState>) -> Response {
    let has_password = match state.auth.owner().await {
        Ok(Some(owner)) => state.auth.has_password(owner.id).await,
        Ok(None) => Ok(false),
        Err(e) => Err(e),
    };
    let has_password = match has_password {
        Ok(has) => has,
        Err(e) => return internal(&e),
    };
    let completed = match state.control.as_deref() {
        Some(control) => match control.setup_completed().await {
            Ok(done) => done,
            Err(e) => return control_error(e),
        },
        None => has_password,
    };
    Json(SetupStatus {
        owner_has_password: has_password,
        completed,
    })
    .into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/setup/owner",
    security(()),
    request_body = SetupOwner,
    description = "Sets the owner's first password and signs them in with a session cookie. Works once, while the owner has no password. Wrong tokens count toward the login rate limit.",
    responses(
        (status = 200, body = UserDto),
        (status = 400, body = ApiError, description = "The password is too short or too long"),
        (status = 401, body = ApiError, description = "Wrong setup token"),
        (status = 409, body = ApiError, description = "The owner already has a password"),
        (status = 429, body = ApiError),
        (status = 503, body = ApiError)
    )
)]
pub(crate) async fn claim_owner(
    State(state): State<ApiState>,
    connect: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(body): Json<SetupOwner>,
) -> Response {
    // Held throughout, so two requests can't both find the owner without a password.
    let mut token = state.setup_token.lock().await;
    let owner = match state.auth.owner().await {
        Ok(Some(owner)) => owner,
        Ok(None) => return error(StatusCode::CONFLICT, "the daemon has no owner yet"),
        Err(e) => return internal(&e),
    };
    match state.auth.has_password(owner.id).await {
        Ok(false) => {}
        Ok(true) => {
            return error(
                StatusCode::CONFLICT,
                "the owner already has a password; sign in instead",
            );
        }
        Err(e) => return internal(&e),
    }
    let client = client(connect.as_ref().map(|Extension(c)| c));
    if let Err(wait) = state.limiter.attempt(client, SETUP_ACCOUNT, Instant::now()) {
        return too_many(wait);
    }
    // Hashes compare in the same time whatever the guess, unlike the strings themselves.
    let digest = |text: &str| fangst_core::secret::sha256_hex(text.trim().as_bytes());
    let token_matches = token
        .as_deref()
        .is_some_and(|expected| digest(expected) == digest(&body.token));
    let key_matches = match state.auth.user_for_api_key(body.token.trim()).await {
        Ok(user) => user.is_some_and(|u| u.id == owner.id),
        Err(e) => return internal(&e),
    };
    if !token_matches && !key_matches {
        return error(StatusCode::UNAUTHORIZED, "wrong setup token");
    }
    state
        .limiter
        .succeeded(client, SETUP_ACCOUNT, Instant::now());
    match state.auth.set_password(owner.id, &body.password).await {
        Ok(()) => {}
        Err(QueueError::Invalid(why)) => return error(StatusCode::BAD_REQUEST, why),
        Err(QueueError::Busy) => return busy(),
        Err(e) => return internal(&e),
    }
    *token = None;
    let session = match state
        .auth
        .start_session(owner.id, state.settings.session_lifetime)
        .await
    {
        Ok(session) => session,
        Err(e) => return internal(&e),
    };
    let mut response = Json(UserDto {
        id: owner.id,
        username: owner.username,
        has_password: true,
    })
    .into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        set_cookie(&state.settings, &session, state.settings.session_lifetime),
    );
    response
}

#[utoipa::path(
    post,
    path = "/api/v1/setup/complete",
    description = "Marks the first-run setup as done, so the web UI stops showing it.",
    responses(
        (status = 204),
        (status = 401, body = ApiError),
        (status = 403, body = ApiError, description = "The caller is not the owner")
    )
)]
pub(crate) async fn complete(
    State(state): State<ApiState>,
    Extension(caller): Extension<Caller>,
) -> Response {
    if let Err(response) = owner_only(&state, &caller).await {
        return response;
    }
    let Some(control) = state.control.as_deref() else {
        return StatusCode::NO_CONTENT.into_response();
    };
    match control.complete_setup().await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => control_error(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/settings/folders/check",
    request_body = FolderCheckRequest,
    description = "Creates the folders if missing, checks that the daemon can write to them, and whether a file can be hardlinked from the downloads folder into the media folder. Nothing is saved.",
    responses(
        (status = 200, body = FolderCheckResult),
        (status = 401, body = ApiError),
        (status = 403, body = ApiError, description = "The caller is not the owner")
    )
)]
pub(crate) async fn check_folders(
    State(state): State<ApiState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<FolderCheckRequest>,
) -> Response {
    if let Err(response) = owner_only(&state, &caller).await {
        return response;
    }
    let Some(control) = state.control.as_deref() else {
        return crate::settings::no_control();
    };
    match control.check_folders(request).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => control_error(e),
    }
}
