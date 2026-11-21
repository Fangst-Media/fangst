//! `GET` and `PUT /api/v1/settings`: the daemon's settings, for its owner only.
//!
//! The API only describes and checks requests; the binary that wires the daemon together reads
//! and writes the config file and applies changes, through [`Control`].

use std::future::Future;
use std::pin::Pin;

use axum::Json;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::Caller;
use crate::{ApiError, ApiState, error, internal};

pub type ControlFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    /// The request breaks a rule; the message says which and is safe to show.
    Invalid(String),
    /// Reading or writing failed; the message goes to the daemon's log only.
    Internal(String),
}

/// Reads, checks, stores and applies settings. Implemented by the daemon binary.
pub trait Control: Send + Sync {
    fn settings(&self) -> ControlFuture<'_, Result<SettingsDto, ControlError>>;

    /// Checks `settings` as a whole, stores them and applies what can change while running.
    fn save_settings(
        &self,
        settings: SettingsDto,
    ) -> ControlFuture<'_, Result<SavedSettings, ControlError>>;

    /// Whether the first-run setup finished or the environment configured everything.
    fn setup_completed(&self) -> ControlFuture<'_, Result<bool, ControlError>>;

    fn complete_setup(&self) -> ControlFuture<'_, Result<(), ControlError>>;

    /// Checks the folders without saving them.
    fn check_folders(
        &self,
        request: crate::FolderCheckRequest,
    ) -> ControlFuture<'_, Result<crate::FolderCheckResult, ControlError>>;
}

/// Every setting the web UI manages.
#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct SettingsDto {
    pub folders: FolderSettings,
    /// In the order they are tried; `priority` sorts them first.
    pub servers: Vec<ServerSettings>,
    pub categories: Vec<CategorySettings>,
    pub notifiers: NotifierSettings,
    pub torrent: TorrentSettings,
    /// Adapters and post-processors; notifiers are switched on under `notifiers`.
    pub providers: Vec<ProviderSettings>,
    /// Dotted names, such as `torrent.listen_port`, of settings an environment variable or a
    /// command-line flag sets. Ignored in requests; a request that changes one is refused.
    #[serde(default)]
    pub locked: Vec<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct FolderSettings {
    /// Where jobs download to. Relative paths are inside the data folder. Takes a restart.
    pub downloads: String,
    /// The media library the downloads are moved or linked into, if any.
    pub media: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SettingSource {
    /// The config file; editable.
    #[default]
    File,
    /// `FANGST_USENET_*` variables; read-only here.
    Environment,
    /// `fangst-dl serve --host` and friends; read-only here.
    Flags,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // Independent settings, as the config file has them.
pub struct ServerSettings {
    /// Keeps the stored password with the server across renames. `null` for a new server.
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub username: Option<String>,
    /// Write-only. Leave out or `null` to keep the stored password, `""` to remove it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Read-only: whether a password is stored or comes from `password_env`.
    #[serde(default)]
    pub has_password: bool,
    /// Environment variable to read the password from instead.
    #[serde(default)]
    pub password_env: Option<String>,
    pub connections: usize,
    /// Lower is tried first.
    #[serde(default)]
    pub priority: i64,
    /// Only asked for segments every primary server is missing.
    #[serde(default)]
    pub backfill: bool,
    pub enabled: bool,
    /// Read-only. Servers not from the file can't be changed here and are ignored in requests.
    #[serde(default)]
    pub source: SettingSource,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct CategorySettings {
    pub name: String,
    /// Folder for the category's new jobs; relative paths are inside the downloads folder.
    pub folder: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct NotifierSettings {
    pub webhook: WebhookSettings,
    pub ntfy: NtfySettings,
    pub discord: DiscordSettings,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct WebhookSettings {
    pub enabled: bool,
    /// `http` or `https`; every event is posted to it as JSON.
    pub url: Option<String>,
    /// Write-only bearer token. Leave out or `null` to keep it, `""` to remove it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default)]
    pub has_token: bool,
    #[serde(default)]
    pub token_env: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct NtfySettings {
    pub enabled: bool,
    /// `null` for the default server.
    pub server: Option<String>,
    pub topic: Option<String>,
    /// Write-only access token for protected topics. `""` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default)]
    pub has_token: bool,
    #[serde(default)]
    pub token_env: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct DiscordSettings {
    pub enabled: bool,
    /// Write-only, since the URL carries the token that lets anyone post. `""` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub has_webhook_url: bool,
    #[serde(default)]
    pub webhook_url_env: Option<String>,
}

/// The torrent engine. Every change here takes a restart.
#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // Independent settings, as the config file has them.
pub struct TorrentSettings {
    pub enabled: bool,
    pub listen_port: u16,
    /// `null` to announce `listen_port`.
    pub announce_port: Option<u16>,
    pub dht: bool,
    pub local_discovery: bool,
    pub upnp: bool,
    /// Bytes per second; `null` for no limit.
    pub download_limit: Option<u32>,
    pub upload_limit: Option<u32>,
    pub max_active: usize,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct ProviderSettings {
    pub id: String,
    /// `api_adapter` or `post_processor`.
    pub kind: String,
    #[serde(default)]
    pub description: String,
    pub enabled: bool,
    /// Read-only: on whatever the request says, because the daemon or a flag needs it.
    #[serde(default)]
    pub required: bool,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, PartialEq, Eq)]
pub struct SavedSettings {
    pub settings: SettingsDto,
    /// Some changes only take effect after the daemon restarts.
    pub restart_required: bool,
    /// What waits for the restart, in words.
    pub restart_reasons: Vec<String>,
}

/// Settings belong to the owner, the first user; other users may only use the queue.
pub(crate) async fn owner_only(state: &ApiState, caller: &Caller) -> Result<(), Response> {
    match state.auth.is_owner(caller.user.id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(error(
            StatusCode::FORBIDDEN,
            "only the owner can see and change settings",
        )),
        Err(e) => Err(internal(&e)),
    }
}

pub(crate) fn no_control() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "settings can't be changed in this setup",
    )
}

pub(crate) fn control_error(e: ControlError) -> Response {
    match e {
        ControlError::Invalid(why) => error(StatusCode::BAD_REQUEST, why),
        ControlError::Internal(why) => {
            eprintln!("error: {why}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error; the daemon's log has the details",
            )
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/settings",
    responses(
        (status = 200, body = SettingsDto),
        (status = 401, body = ApiError),
        (status = 403, body = ApiError, description = "The caller is not the owner")
    )
)]
pub(crate) async fn get_settings(
    State(state): State<ApiState>,
    Extension(caller): Extension<Caller>,
) -> Response {
    if let Err(response) = owner_only(&state, &caller).await {
        return response;
    }
    let Some(control) = state.control.as_deref() else {
        return no_control();
    };
    match control.settings().await {
        Ok(settings) => Json(settings).into_response(),
        Err(e) => control_error(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/settings",
    request_body = SettingsDto,
    description = "Replaces every setting. Servers, categories, notifiers and adapters change at once; the response says what waits for a restart. Passwords and tokens left out or `null` stay as stored.",
    responses(
        (status = 200, body = SavedSettings),
        (status = 400, body = ApiError, description = "A setting is invalid or set by the environment"),
        (status = 401, body = ApiError),
        (status = 403, body = ApiError, description = "The caller is not the owner")
    )
)]
pub(crate) async fn put_settings(
    State(state): State<ApiState>,
    Extension(caller): Extension<Caller>,
    Json(settings): Json<SettingsDto>,
) -> Response {
    if let Err(response) = owner_only(&state, &caller).await {
        return response;
    }
    let Some(control) = state.control.as_deref() else {
        return no_control();
    };
    match control.save_settings(settings).await {
        Ok(saved) => Json(saved).into_response(),
        Err(e) => control_error(e),
    }
}
