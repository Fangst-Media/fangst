//! The settings routes: owner only, and a thin pass-through to the daemon's [`Control`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use fangst_core::secret::MasterKey;
use fangst_dl_api::{
    AuthSettings, Control, ControlError, ControlFuture, DiscordSettings, EnabledAdapters,
    FetchLimits, FolderCheckRequest, FolderCheckResult, FolderSettings, FolderStatus,
    NotifierSettings, NtfySettings, Options, SavedSettings, SettingsDto, TorrentSettings,
    WebhookSettings, app,
};
use fangst_downloads::{Auth, Runner, Service, Store};
use fangst_providers::{
    ApiAdapter, BoxFuture, Credential, HttpRequest, HttpResponse, Jobs, ProviderInfo, ProviderKind,
};
use http_body_util::BodyExt;
use tower::ServiceExt;

// Placeholders only; never real credentials.
const OWNER_KEY: &str = "owner-key-placeholder-1";
const OTHER_KEY: &str = "other-key-placeholder-2";

fn sample() -> SettingsDto {
    SettingsDto {
        folders: FolderSettings {
            downloads: "downloads".to_owned(),
            media: None,
        },
        servers: Vec::new(),
        categories: Vec::new(),
        notifiers: NotifierSettings {
            webhook: WebhookSettings {
                enabled: false,
                url: None,
                token: None,
                has_token: false,
                token_env: None,
            },
            ntfy: NtfySettings {
                enabled: false,
                server: None,
                topic: None,
                token: None,
                has_token: false,
                token_env: None,
            },
            discord: DiscordSettings {
                enabled: false,
                webhook_url: None,
                has_webhook_url: false,
                webhook_url_env: None,
            },
        },
        torrent: TorrentSettings {
            enabled: true,
            listen_port: 6881,
            announce_port: None,
            dht: true,
            local_discovery: true,
            upnp: true,
            download_limit: None,
            upload_limit: None,
            max_active: 5,
        },
        providers: Vec::new(),
        locked: Vec::new(),
    }
}

/// Keeps what it was given and refuses a downloads folder named `bad`.
#[derive(Default)]
struct Fake {
    saved: Mutex<Option<SettingsDto>>,
    completed: AtomicBool,
}

impl Control for Fake {
    fn settings(&self) -> ControlFuture<'_, Result<SettingsDto, ControlError>> {
        Box::pin(async move { Ok(self.saved.lock().unwrap().clone().unwrap_or_else(sample)) })
    }

    fn save_settings(
        &self,
        settings: SettingsDto,
    ) -> ControlFuture<'_, Result<SavedSettings, ControlError>> {
        Box::pin(async move {
            match settings.folders.downloads.as_str() {
                "bad" => return Err(ControlError::Invalid("folders.downloads is bad".to_owned())),
                "broken" => {
                    return Err(ControlError::Internal(
                        "C:\\secret\\path: disk on fire".to_owned(),
                    ));
                }
                _ => {}
            }
            *self.saved.lock().unwrap() = Some(settings.clone());
            Ok(SavedSettings {
                settings,
                restart_required: true,
                restart_reasons: vec!["the downloads folder".to_owned()],
            })
        })
    }

    fn setup_completed(&self) -> ControlFuture<'_, Result<bool, ControlError>> {
        Box::pin(async move { Ok(self.completed.load(Ordering::SeqCst)) })
    }

    fn complete_setup(&self) -> ControlFuture<'_, Result<(), ControlError>> {
        Box::pin(async move {
            self.completed.store(true, Ordering::SeqCst);
            Ok(())
        })
    }

    fn check_folders(
        &self,
        request: FolderCheckRequest,
    ) -> ControlFuture<'_, Result<FolderCheckResult, ControlError>> {
        Box::pin(async move {
            Ok(FolderCheckResult {
                downloads: FolderStatus {
                    path: request.downloads,
                    created: false,
                    writable: true,
                    problem: None,
                },
                media: None,
                hardlinks: None,
                hardlink_problem: None,
            })
        })
    }
}

struct Probe;

impl ApiAdapter for Probe {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "adapter-probe".to_owned(),
            kind: ProviderKind::ApiAdapter,
            description: "test".to_owned(),
        }
    }

    fn mount_path(&self) -> String {
        "/probe".to_owned()
    }

    fn credential(&self, _request: &HttpRequest) -> Credential {
        Credential::NotNeeded
    }

    fn rejected(&self, _request: &HttpRequest, _missing: bool) -> HttpResponse {
        HttpResponse::text(401, "no")
    }

    fn handle(&self, _request: HttpRequest, _jobs: Arc<dyn Jobs>) -> BoxFuture<HttpResponse> {
        Box::pin(async { HttpResponse::text(200, "probe") })
    }
}

struct Harness {
    app: axum::Router,
    enabled: EnabledAdapters,
    _dir: tempfile::TempDir,
}

async fn harness(control: Option<Arc<dyn Control>>) -> Harness {
    harness_with(control, false).await
}

const SETUP_TOKEN: &str = "setup-token-placeholder";
const OWNER_PASSWORD: &str = "placeholder-password";

async fn harness_with(control: Option<Arc<dyn Control>>, owner_password: bool) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let auth = Auth::new(
        &store,
        Arc::new(MasterKey::load_or_create(&dir.path().join("master.key")).unwrap()),
    )
    .await
    .unwrap();
    auth.bootstrap(OWNER_KEY).await.unwrap();
    let other = auth.create_user("someone", None).await.unwrap();
    auth.set_api_key(other.id, "cli", OTHER_KEY).await.unwrap();
    if owner_password {
        let owner = auth.owner().await.unwrap().unwrap();
        auth.set_password(owner.id, OWNER_PASSWORD).await.unwrap();
    }
    let runner = Runner {
        store,
        servers: Vec::new(),
        root: dir.path().join("downloads"),
        options: fangst_usenet::Options {
            connections: 1,
            pipeline_depth: 1,
            retries: 0,
        },
        post_processors: Vec::new(),
    };
    let enabled = EnabledAdapters::default();
    let app = app(
        Service::new(runner).without_usenet(),
        auth,
        Options {
            auth: AuthSettings::default(),
            fetch: FetchLimits::default(),
            adapters: vec![Arc::new(Probe)],
            enabled_adapters: enabled.clone(),
            control,
            setup_token: Some(SETUP_TOKEN.to_owned()),
        },
    );
    Harness {
        app,
        enabled,
        _dir: dir,
    }
}

async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    key: Option<&str>,
    body: Option<&SettingsDto>,
) -> (StatusCode, String) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(key) = key {
        request = request.header("X-Api-Key", key);
    }
    let body = match body {
        Some(body) => {
            request = request.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(body).unwrap())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn only_the_owner_reads_and_changes_settings() {
    let h = harness(Some(Arc::new(Fake::default()))).await;
    let (status, _) = call(&h.app, "GET", "/api/v1/settings", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = call(&h.app, "GET", "/api/v1/settings", Some(OTHER_KEY), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, _) = call(
        &h.app,
        "PUT",
        "/api/v1/settings",
        Some(OTHER_KEY),
        Some(&sample()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = call(&h.app, "GET", "/api/v1/settings", Some(OWNER_KEY), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        serde_json::from_str::<SettingsDto>(&body).unwrap(),
        sample()
    );
}

#[tokio::test]
async fn saving_reports_restarts_and_refusals() {
    let h = harness(Some(Arc::new(Fake::default()))).await;
    let mut changed = sample();
    changed.folders.downloads = "elsewhere".to_owned();
    let (status, body) = call(
        &h.app,
        "PUT",
        "/api/v1/settings",
        Some(OWNER_KEY),
        Some(&changed),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let saved: SavedSettings = serde_json::from_str(&body).unwrap();
    assert!(saved.restart_required);
    assert_eq!(saved.settings, changed);

    changed.folders.downloads = "bad".to_owned();
    let (status, body) = call(
        &h.app,
        "PUT",
        "/api/v1/settings",
        Some(OWNER_KEY),
        Some(&changed),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("folders.downloads is bad"), "{body}");

    changed.folders.downloads = "broken".to_owned();
    let (status, body) = call(
        &h.app,
        "PUT",
        "/api/v1/settings",
        Some(OWNER_KEY),
        Some(&changed),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body.contains("secret"), "internal detail leaked: {body}");
}

#[tokio::test]
async fn without_a_control_the_settings_routes_say_so() {
    let h = harness(None).await;
    let (status, _) = call(&h.app, "GET", "/api/v1/settings", Some(OWNER_KEY), None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

async fn post_json(
    app: &axum::Router,
    path: &str,
    key: Option<&str>,
    json: &str,
) -> (StatusCode, Option<String>, String) {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("Content-Type", "application/json");
    if let Some(key) = key {
        request = request.header("X-Api-Key", key);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(json.to_owned())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get("set-cookie")
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, cookie, String::from_utf8_lossy(&bytes).into_owned())
}

fn claim(token: &str, password: &str) -> String {
    serde_json::json!({ "token": token, "password": password }).to_string()
}

#[tokio::test]
async fn setup_status_needs_no_credentials_and_follows_the_owner() {
    let h = harness(Some(Arc::new(Fake::default()))).await;
    let (status, body) = call(&h.app, "GET", "/api/v1/setup", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({ "owner_has_password": false, "completed": false })
    );
    let (status, _, _) = post_json(&h.app, "/api/v1/setup/complete", Some(OTHER_KEY), "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = post_json(&h.app, "/api/v1/setup/complete", Some(OWNER_KEY), "").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = call(&h.app, "GET", "/api/v1/setup", None, None).await;
    assert!(body.contains(r#""completed":true"#), "{body}");
}

#[tokio::test]
async fn the_setup_token_claims_the_owner_once_and_signs_them_in() {
    let h = harness(Some(Arc::new(Fake::default()))).await;
    let wrong = claim("not-the-token", OWNER_PASSWORD);
    let (status, cookie, body) = post_json(&h.app, "/api/v1/setup/owner", None, &wrong).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert!(cookie.is_none());

    let short = claim(SETUP_TOKEN, "short");
    let (status, _, body) = post_json(&h.app, "/api/v1/setup/owner", None, &short).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let good = claim(SETUP_TOKEN, OWNER_PASSWORD);
    let (status, cookie, body) = post_json(&h.app, "/api/v1/setup/owner", None, &good).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(r#""username":"admin""#), "{body}");
    assert!(cookie.unwrap().starts_with("fangst_session="));

    let again = claim(SETUP_TOKEN, "another-placeholder");
    let (status, _, _) = post_json(&h.app, "/api/v1/setup/owner", None, &again).await;
    assert_eq!(status, StatusCode::CONFLICT, "works once");
    let (_, body) = call(&h.app, "GET", "/api/v1/setup", None, None).await;
    assert!(body.contains(r#""owner_has_password":true"#), "{body}");
}

#[tokio::test]
async fn the_owners_api_key_also_claims_the_account_but_another_users_does_not() {
    let h = harness(Some(Arc::new(Fake::default()))).await;
    let other = claim(OTHER_KEY, OWNER_PASSWORD);
    let (status, _, _) = post_json(&h.app, "/api/v1/setup/owner", None, &other).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let owner = claim(OWNER_KEY, OWNER_PASSWORD);
    let (status, _, body) = post_json(&h.app, "/api/v1/setup/owner", None, &owner).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn an_owner_with_a_password_signs_in_instead() {
    let h = harness_with(Some(Arc::new(Fake::default())), true).await;
    let again = claim(SETUP_TOKEN, "another-placeholder");
    let (status, _, _) = post_json(&h.app, "/api/v1/setup/owner", None, &again).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn only_the_owner_checks_folders() {
    let h = harness(Some(Arc::new(Fake::default()))).await;
    let request = r#"{"downloads":"downloads","media":null}"#;
    let path = "/api/v1/settings/folders/check";
    let (status, _, _) = post_json(&h.app, path, Some(OTHER_KEY), request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, body) = post_json(&h.app, path, Some(OWNER_KEY), request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(r#""writable":true"#), "{body}");
}

#[tokio::test]
async fn adapters_answer_only_while_enabled() {
    let h = harness(None).await;
    let (status, _) = call(&h.app, "GET", "/probe/x", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    h.enabled.set(["adapter-probe".to_owned()]);
    let (status, body) = call(&h.app, "GET", "/probe/x", None, None).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "probe"));
    h.enabled.set([]);
    let (status, _) = call(&h.app, "GET", "/probe/x", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
