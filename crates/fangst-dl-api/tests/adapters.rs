//! NZB by URL on the native API, and how adapters are mounted and authenticated.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use fangst_core::secret::MasterKey;
use fangst_dl_api::{AuthSettings, FetchLimits, JobDto, router_with_adapters};
use fangst_downloads::{Auth, Runner, Service, Store};
use fangst_providers::{
    ApiAdapter, BoxFuture, Credential, HttpRequest, HttpResponse, Jobs, ProviderInfo, ProviderKind,
};
use http_body_util::BodyExt;
use tower::ServiceExt;

// Placeholder only; never a real credential.
const KEY: &str = "adapter-test-key-placeholder";

/// Takes `?key=`, lets `/open` through without one, and answers with the job count.
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
        "/api".to_owned()
    }

    fn credential(&self, request: &HttpRequest) -> Credential {
        if request.path == "/open" {
            return Credential::NotNeeded;
        }
        request
            .query("key")
            .map_or(Credential::Missing, |k| Credential::ApiKey(k.to_owned()))
    }

    fn rejected(&self, _request: &HttpRequest, missing: bool) -> HttpResponse {
        HttpResponse::text(200, if missing { "missing" } else { "wrong" })
    }

    fn handle(&self, request: HttpRequest, jobs: Arc<dyn Jobs>) -> BoxFuture<HttpResponse> {
        Box::pin(async move {
            let count = jobs.list().await.map_or(usize::MAX, |j| j.len());
            HttpResponse::text(
                200,
                format!("{} {} jobs={count}", request.method, request.path),
            )
        })
    }
}

struct Harness {
    app: axum::Router,
    nzb: String,
    _dir: tempfile::TempDir,
}

async fn harness(adapters: &[Arc<dyn ApiAdapter>], fetch: FetchLimits) -> Harness {
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
        options: fangst_usenet::Options {
            connections: 1,
            pipeline_depth: 1,
            retries: 0,
        },
    };
    let mut library = fangst_bench::Library::new();
    library.add_file("a.bin", &[7u8; 2_000], 1_000, "g");
    Harness {
        app: router_with_adapters(
            Service::new(runner),
            auth,
            AuthSettings::default(),
            fetch,
            adapters,
        ),
        nzb: library.nzb(),
        _dir: dir,
    }
}

async fn call(app: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

fn add_url(url: &str) -> Request<Body> {
    Request::post("/api/v1/jobs?category=tv")
        .header("x-api-key", KEY)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({ "url": url }).to_string()))
        .unwrap()
}

/// Serves `routes` on a free local port until the test ends.
async fn web(routes: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, routes).await });
    format!("http://{addr}")
}

fn nzb_routes(nzb: String) -> axum::Router {
    use axum::routing::get;
    let plain = nzb.clone();
    axum::Router::new()
        .route(
            "/get/Some.Show.S01E01.nzb",
            get(move || async move { plain }),
        )
        .route(
            "/titled",
            get(move || async move {
                (
                    [(
                        "content-disposition",
                        "attachment; filename=\"From.Header.nzb\"",
                    )],
                    nzb,
                )
            }),
        )
        .route(
            "/hop",
            get(|| async { axum::response::Redirect::temporary("/get/Some.Show.S01E01.nzb") }),
        )
        .route(
            "/to-file",
            get(|| async { axum::response::Redirect::temporary("file:///etc/passwd") }),
        )
        .route(
            "/loop",
            get(|| async { axum::response::Redirect::temporary("/loop") }),
        )
        .route(
            "/missing",
            get(|| async { StatusCode::NOT_FOUND.into_response() }),
        )
        .route("/big", get(|| async { vec![b'x'; 4_096] }))
        .route(
            "/big-stream",
            get(|| async {
                let chunks = futures_util::stream::iter(
                    (0..8).map(|_| Ok::<_, std::io::Error>(vec![b'x'; 1_024])),
                );
                Body::from_stream(chunks)
            }),
        )
}

fn small() -> FetchLimits {
    FetchLimits {
        max_bytes: 3_000,
        timeout: Duration::from_secs(5),
        max_redirects: 3,
    }
}

#[tokio::test]
async fn a_job_can_be_added_by_url() {
    let h = harness(&[], FetchLimits::default()).await;
    let base = web(nzb_routes(h.nzb.clone())).await;

    let (status, body) = call(&h.app, add_url(&format!("{base}/get/Some.Show.S01E01.nzb"))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let job: JobDto = serde_json::from_str(&body).unwrap();
    assert_eq!(job.name, "Some.Show.S01E01", "named after the URL's file");
    assert_eq!(job.category.as_deref(), Some("tv"));

    let (status, body) = call(&h.app, add_url(&format!("{base}/titled"))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let job: JobDto = serde_json::from_str(&body).unwrap();
    assert_eq!(job.name, "From.Header");

    let (status, body) = call(&h.app, add_url(&format!("{base}/hop"))).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "redirects to http are followed: {body}"
    );
}

#[tokio::test]
async fn only_http_urls_are_fetched() {
    let h = harness(&[], small()).await;
    let base = web(nzb_routes(h.nzb.clone())).await;

    for url in ["file:///etc/passwd", "ftp://example.com/a.nzb", "not a url"] {
        let (status, body) = call(&h.app, add_url(url)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}: {body}");
    }
    let (status, body) = call(&h.app, add_url(&format!("{base}/to-file"))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert!(body.contains("not http or https"), "{body}");
    let (status, body) = call(&h.app, add_url(&format!("{base}/loop"))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert!(body.contains("too many redirects"), "{body}");
    let (status, body) = call(&h.app, add_url(&format!("{base}/missing"))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert!(body.contains("HTTP 404"), "{body}");
}

#[tokio::test]
async fn oversize_nzbs_are_refused_with_or_without_a_length() {
    let h = harness(&[], small()).await;
    let base = web(nzb_routes(h.nzb.clone())).await;
    for path in ["/big", "/big-stream"] {
        let (status, body) = call(&h.app, add_url(&format!("{base}{path}"))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{path}: {body}");
        assert!(body.contains("larger than 3000 bytes"), "{body}");
    }
}

#[tokio::test]
async fn adapters_check_their_credential_against_user_keys() {
    let h = harness(&[Arc::new(Probe)], FetchLimits::default()).await;
    assert_eq!(call(&h.app, get("/api")).await.1, "missing");
    assert_eq!(call(&h.app, get("/api?key=nope")).await.1, "wrong");
    assert_eq!(
        call(&h.app, get(&format!("/api?key={KEY}"))).await.1,
        "GET / jobs=0"
    );
    assert_eq!(
        call(&h.app, get(&format!("/api/deeper/path?key={KEY}")))
            .await
            .1,
        "GET /deeper/path jobs=0"
    );
    assert_eq!(call(&h.app, get("/api/open")).await.1, "GET /open jobs=0");
}

#[tokio::test]
async fn an_adapter_at_api_leaves_the_native_api_alone() {
    let h = harness(&[Arc::new(Probe)], FetchLimits::default()).await;
    let (status, _) = call(&h.app, get("/api/v1/health")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&h.app, get("/api/v1/jobs")).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "native auth still applies"
    );
    let (status, body) = call(&h.app, get(&format!("/api/v1/nothing?key={KEY}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn adapters_are_not_mounted_unless_enabled() {
    let h = harness(&[], FetchLimits::default()).await;
    let (status, _) = call(&h.app, get(&format!("/api?key={KEY}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
