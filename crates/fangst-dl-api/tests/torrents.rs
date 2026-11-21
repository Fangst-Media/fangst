//! Torrents through `POST /api/v1/jobs`: `.torrent` bodies, magnet links in JSON or the query, and
//! URLs that answer with a `.torrent` or redirect to a magnet link. The service runs no engine, so
//! jobs only queue.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use fangst_core::secret::MasterKey;
use fangst_dl_api::{AuthSettings, FetchLimits, JobDto, JobKind, router_with_adapters};
use fangst_downloads::{Auth, Runner, Service, Store};
use http_body_util::BodyExt;
use tower::ServiceExt;

// Placeholder only; never a real credential.
const KEY: &str = "torrent-test-key-placeholder";
const HASH: &str = "cab507494d02ebb1178b38f2e9d7be299c86b862";

struct Harness {
    app: axum::Router,
    torrent: Vec<u8>,
    _dir: tempfile::TempDir,
}

async fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let auth = Auth::new(
        &store,
        Arc::new(MasterKey::load_or_create(&dir.path().join("master.key")).unwrap()),
    )
    .await
    .unwrap();
    auth.bootstrap(KEY).await.unwrap();
    let runner = Runner {
        store,
        servers: vec![fangst_usenet::Server::primary(Arc::new(
            fangst_nntp::Pool::new(fangst_nntp::ServerConfig {
                host: "localhost".to_owned(),
                port: 0,
                tls: None,
                credentials: None,
                max_connections: 1,
                connect_timeout: Duration::from_secs(1),
                command_timeout: Duration::from_secs(1),
            }),
        ))],
        root: dir.path().join("downloads"),
        post_processors: Vec::new(),
        options: fangst_usenet::Options {
            connections: 1,
            pipeline_depth: 1,
            retries: 0,
        },
    };
    let release = dir.path().join("source").join("Some.Release");
    std::fs::create_dir_all(&release).unwrap();
    std::fs::write(release.join("a.bin"), vec![3u8; 40_000]).unwrap();
    let torrent = fangst_torrent::create_torrent(&release, Vec::new(), None)
        .await
        .unwrap();
    Harness {
        app: router_with_adapters(
            Service::new(runner),
            auth,
            AuthSettings::default(),
            FetchLimits::default(),
            &[],
        ),
        torrent,
        _dir: dir,
    }
}

async fn post(
    h: &Harness,
    uri: &str,
    content_type: Option<&str>,
    body: impl Into<Body>,
) -> (StatusCode, String) {
    let mut request = Request::post(uri).header("x-api-key", KEY);
    if let Some(content_type) = content_type {
        request = request.header("content-type", content_type);
    }
    let response = h
        .app
        .clone()
        .oneshot(request.body(body.into()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

fn job(status: StatusCode, body: &str) -> JobDto {
    assert_eq!(status, StatusCode::CREATED, "{body}");
    serde_json::from_str(body).unwrap()
}

fn json(value: &serde_json::Value) -> String {
    value.to_string()
}

#[tokio::test]
async fn a_torrent_file_becomes_a_torrent_job() {
    let h = harness().await;
    let (status, body) = post(
        &h,
        "/api/v1/jobs?category=tv&priority=3",
        Some("application/x-bittorrent"),
        h.torrent.clone(),
    )
    .await;
    let added = job(status, &body);
    assert_eq!(added.kind, JobKind::Torrent);
    assert_eq!(added.name, "Some.Release", "named after the torrent");
    assert_eq!(added.total_bytes, 40_000);
    assert_eq!(added.category.as_deref(), Some("tv"));
    assert_eq!(added.priority, 3);

    let (status, body) = post(
        &h,
        "/api/v1/jobs",
        Some("application/x-bittorrent"),
        h.torrent.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    let (status, body) = post(
        &h,
        "/api/v1/jobs",
        Some("application/x-bittorrent"),
        "not bencode",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
}

#[tokio::test]
async fn magnet_links_come_as_json_or_in_the_query() {
    let h = harness().await;
    let link = format!("magnet:?xt=urn:btih:{HASH}&dn=From.Json");
    let (status, body) = post(
        &h,
        "/api/v1/jobs",
        Some("application/json"),
        json(&serde_json::json!({ "magnet": link })),
    )
    .await;
    let added = job(status, &body);
    assert_eq!(added.kind, JobKind::Torrent);
    assert_eq!(added.name, "From.Json", "named after the link's dn");
    assert_eq!(
        added.total_bytes, 0,
        "unknown until peers send the metadata"
    );

    let other = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567";
    let uri = format!(
        "/api/v1/jobs?name=Named&magnet={}",
        form_urlencoded::byte_serialize(other.as_bytes()).collect::<String>()
    );
    let added = {
        let (status, body) = post(&h, &uri, None, Body::empty()).await;
        job(status, &body)
    };
    assert_eq!(added.name, "Named");

    // A magnet link in `url` is what a user pasting into the wrong field means.
    let third = "magnet:?xt=urn:btih:1111111111111111111111111111111111111111";
    let (status, body) = post(
        &h,
        "/api/v1/jobs",
        Some("application/json"),
        json(&serde_json::json!({ "url": third })),
    )
    .await;
    assert_eq!(job(status, &body).kind, JobKind::Torrent);

    let (status, body) = post(
        &h,
        "/api/v1/jobs",
        Some("application/json"),
        json(&serde_json::json!({ "magnet": link })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    for bad in ["https://example.com/a.torrent", "magnet:?dn=no-hash"] {
        let (status, body) = post(
            &h,
            "/api/v1/jobs",
            Some("application/json"),
            json(&serde_json::json!({ "magnet": bad })),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
    }
    let (status, body) = post(
        &h,
        "/api/v1/jobs",
        Some("application/json"),
        json(&serde_json::json!({ "magnet": link, "url": "https://example.com/a.nzb" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

/// Serves `routes` on a free local port until the test ends.
async fn web(routes: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, routes).await });
    format!("http://{addr}")
}

#[tokio::test]
async fn a_url_may_answer_with_a_torrent_or_redirect_to_a_magnet_link() {
    use axum::routing::get;
    let h = harness().await;
    let torrent = h.torrent.clone();
    let base = web(axum::Router::new()
        .route(
            "/dl/Some.Release.torrent",
            get(move || async move { ([("content-type", "application/x-bittorrent")], torrent) }),
        )
        .route(
            "/magnet",
            get(|| async {
                axum::response::Redirect::temporary(&format!(
                    "magnet:?xt=urn:btih:{HASH}&dn=Redirected"
                ))
            }),
        ))
    .await;

    let (status, body) = post(
        &h,
        "/api/v1/jobs",
        Some("application/json"),
        json(&serde_json::json!({ "url": format!("{base}/dl/Some.Release.torrent") })),
    )
    .await;
    let added = job(status, &body);
    assert_eq!(added.kind, JobKind::Torrent);
    assert_eq!(added.total_bytes, 40_000);

    let (status, body) = post(
        &h,
        "/api/v1/jobs",
        Some("application/json"),
        json(&serde_json::json!({ "url": format!("{base}/magnet") })),
    )
    .await;
    let added = job(status, &body);
    assert_eq!(added.kind, JobKind::Torrent);
    assert_eq!(added.name, "Redirected");
}
