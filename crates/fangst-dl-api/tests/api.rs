use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use fangst_bench::{Config, Library, MockServer};
use fangst_core::secret::MasterKey;
use fangst_dl_api::{AuthSettings, ChangeDto, EventDto, JobDto, JobStatus, openapi_json, router};
use fangst_downloads::{Auth, Runner, Service, Store};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use tower::ServiceExt;

const KEY: &str = "test-key-123";

struct Harness {
    service: Service,
    auth: Auth,
    _server: MockServer,
    _dir: tempfile::TempDir,
    nzb: String,
}

async fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::new();
    library.add_file("a.bin", &vec![9u8; 30_000], 10_000, "g");
    let nzb = library.nzb();
    let server = MockServer::start(library, Config::default()).await.unwrap();
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
        pool: Arc::new(fangst_nntp::Pool::new(fangst_nntp::ServerConfig {
            host: "127.0.0.1".to_owned(),
            port: server.addr.port(),
            tls: None,
            credentials: None,
            max_connections: 2,
            connect_timeout: Duration::from_secs(5),
            command_timeout: Duration::from_secs(5),
        })),
        root: dir.path().join("downloads"),
        options: fangst_usenet::Options {
            connections: 2,
            pipeline_depth: 2,
            retries: 1,
        },
    };
    Harness {
        service: Service::new(runner),
        auth,
        _server: server,
        _dir: dir,
        nzb,
    }
}

fn app(h: &Harness) -> axum::Router {
    router(h.service.clone(), h.auth.clone(), AuthSettings::default())
}

async fn call(
    h: &Harness,
    method: &str,
    uri: &str,
    key: Option<&str>,
    body: Body,
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(key) = key {
        request = request.header("x-api-key", key);
    }
    let response = app(h).oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    (
        status,
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
}

async fn add(h: &Harness) -> JobDto {
    let (status, body) = call(
        h,
        "POST",
        "/api/v1/jobs?name=Show&category=tv&priority=5",
        Some(KEY),
        Body::from(h.nzb.clone()),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn health_needs_no_key() {
    let h = harness().await;
    let (status, body) = call(&h, "GET", "/api/v1/health", None, Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&body).contains("\"status\":\"ok\""));
}

#[tokio::test]
async fn everything_else_needs_the_right_key() {
    let h = harness().await;
    for (method, uri) in [
        ("GET", "/api/v1/jobs"),
        ("POST", "/api/v1/jobs"),
        ("GET", "/api/v1/jobs/1"),
        ("DELETE", "/api/v1/jobs/1"),
        ("POST", "/api/v1/jobs/1/pause"),
        ("POST", "/api/v1/jobs/1/resume"),
        ("GET", "/api/v1/jobs/1/timeline"),
        ("GET", "/api/v1/events"),
        ("POST", "/api/v1/servers/test"),
    ] {
        for key in [None, Some("wrong"), Some("test-key-12")] {
            let (status, _) = call(&h, method, uri, key, Body::empty()).await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {uri} with {key:?}"
            );
        }
    }
}

#[tokio::test]
async fn job_lifecycle_through_the_api() {
    let h = harness().await;
    let job = add(&h).await;
    assert_eq!(
        (job.name.as_str(), job.category.as_deref(), job.priority),
        ("Show", Some("tv"), 5)
    );
    assert_eq!(job.status, JobStatus::Queued);

    let (_, body) = call(&h, "GET", "/api/v1/jobs", Some(KEY), Body::empty()).await;
    let jobs: Vec<JobDto> = serde_json::from_slice(&body).unwrap();
    assert_eq!(jobs, std::slice::from_ref(&job));

    let uri = format!("/api/v1/jobs/{}", job.id);
    let (status, body) = call(
        &h,
        "POST",
        &format!("{uri}/pause"),
        Some(KEY),
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<JobDto>(&body).unwrap().status,
        JobStatus::Paused
    );
    let (status, _) = call(
        &h,
        "POST",
        &format!("{uri}/pause"),
        Some(KEY),
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = call(
        &h,
        "POST",
        &format!("{uri}/resume"),
        Some(KEY),
        Body::empty(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = call(
        &h,
        "GET",
        &format!("{uri}/timeline"),
        Some(KEY),
        Body::empty(),
    )
    .await;
    let events: Vec<EventDto> = serde_json::from_slice(&body).unwrap();
    let messages: Vec<&str> = events.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(
        messages,
        ["added with 1 files, 30000 bytes", "paused", "queued"]
    );

    let (status, _) = call(&h, "DELETE", &uri, Some(KEY), Body::empty()).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&h, "GET", &uri, Some(KEY), Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn name_defaults_to_the_nzb_title() {
    let h = harness().await;
    let nzb = h.nzb.replacen(
        "<file ",
        "<head><meta type=\"title\">From The Title</meta></head>\n  <file ",
        1,
    );
    let (status, body) = call(&h, "POST", "/api/v1/jobs", Some(KEY), Body::from(nzb)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        serde_json::from_slice::<JobDto>(&body).unwrap().name,
        "From The Title"
    );
}

#[tokio::test]
async fn invalid_nzb_is_422() {
    let h = harness().await;
    let (status, body) = call(&h, "POST", "/api/v1/jobs", Some(KEY), Body::from("<html/>")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(String::from_utf8_lossy(&body).contains("invalid NZB"));
}

#[tokio::test]
async fn unknown_job_is_404() {
    let h = harness().await;
    for (method, uri) in [
        ("GET", "/api/v1/jobs/99"),
        ("GET", "/api/v1/jobs/99/timeline"),
        ("POST", "/api/v1/jobs/99/pause"),
    ] {
        let (status, _) = call(&h, method, uri, Some(KEY), Body::empty()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn queued_job_runs_to_completion() {
    let h = harness().await;
    let service = h.service.clone();
    let worker = tokio::spawn(async move { service.run().await });
    let job = add(&h).await;
    let mut status = JobStatus::Queued;
    for _ in 0..200 {
        let (_, body) = call(
            &h,
            "GET",
            &format!("/api/v1/jobs/{}", job.id),
            Some(KEY),
            Body::empty(),
        )
        .await;
        status = serde_json::from_slice::<JobDto>(&body).unwrap().status;
        if status == JobStatus::Completed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    worker.abort();
    assert_eq!(status, JobStatus::Completed);
}

#[tokio::test(flavor = "multi_thread")]
async fn event_stream_reports_changes() {
    let h = harness().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = app(&h);
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/v1/events?api_key={KEY}"))
            .await
            .unwrap();
    let id = h
        .service
        .add_nzb("over the wire", None, 0, h.nzb.as_bytes())
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let change: ChangeDto = serde_json::from_str(message.to_text().unwrap()).unwrap();
    match change {
        ChangeDto::Updated { job } => {
            assert_eq!((job.id, job.name.as_str()), (id, "over the wire"));
        }
        ChangeDto::Deleted { .. } => panic!("expected an update"),
    }

    let refused =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/v1/events?api_key=nope")).await;
    assert!(refused.is_err());
    server.abort();
}

#[test]
fn every_route_is_in_the_openapi_document() {
    let doc: serde_json::Value = serde_json::from_str(&openapi_json()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for (path, methods) in [
        ("/api/v1/health", &["get"][..]),
        ("/api/v1/jobs", &["get", "post"]),
        ("/api/v1/jobs/{id}", &["get", "delete"]),
        ("/api/v1/jobs/{id}/pause", &["post"]),
        ("/api/v1/jobs/{id}/resume", &["post"]),
        ("/api/v1/jobs/{id}/timeline", &["get"]),
        ("/api/v1/events", &["get"]),
        ("/api/v1/servers/test", &["post"]),
        ("/api/v1/auth/login", &["post"]),
        ("/api/v1/auth/logout", &["post"]),
        ("/api/v1/auth/me", &["get"]),
        ("/api/v1/auth/password", &["put"]),
    ] {
        for method in methods {
            assert!(
                paths[path][method].is_object(),
                "{method} {path} missing from OpenAPI"
            );
        }
    }
    assert_eq!(
        paths.len(),
        12,
        "a route exists in the document but not in this test"
    );
    assert!(doc["components"]["securitySchemes"]["api_key"].is_object());
    assert!(doc["components"]["securitySchemes"]["session"].is_object());
}

async fn test_server_call(h: &Harness, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/servers/test")
        .header("x-api-key", KEY)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app(h).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn server_test_reports_success_and_causes() {
    let h = harness().await;
    let limited = MockServer::start(
        Library::new(),
        Config {
            max_connections: 2,
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let port = limited.addr.port();

    let (status, ok) = test_server_call(
        &h,
        serde_json::json!({"host": "127.0.0.1", "port": port, "tls": false, "connections": 2}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ok["ok"], true);
    assert_eq!(ok["message"], "connected and logged in with 2 connections");

    let (_, too_many) = test_server_call(
        &h,
        serde_json::json!({"host": "127.0.0.1", "port": port, "tls": false, "connections": 4}),
    )
    .await;
    assert_eq!(too_many["ok"], false);
    assert_eq!(too_many["cause"], "connection_limit");
    assert_eq!(too_many["connections_accepted"], 2);

    let (_, dns) = test_server_call(
        &h,
        serde_json::json!({"host": "nothing.invalid", "port": 119, "tls": false}),
    )
    .await;
    assert_eq!(dns["cause"], "dns");

    let (status, _) = test_server_call(
        &h,
        serde_json::json!({"host": "127.0.0.1", "port": port, "connections": 0}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn event_stream_key_is_url_decoded() {
    let h = harness().await;
    let key = "plus+slash/equals=0123456789";
    let request = Request::builder()
        .uri("/api/v1/events?api_key=plus%2Bslash%2Fequals%3D0123456789")
        .body(Body::empty())
        .unwrap();
    h.auth.bootstrap(key).await.unwrap();
    let response = app(&h).oneshot(request).await.unwrap();
    // Past the key check the handler wants a WebSocket upgrade, which this plain request isn't.
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn query_parameters_are_documented_as_query() {
    let doc: serde_json::Value = serde_json::from_str(&openapi_json()).unwrap();
    for (path, method, name) in [
        ("/api/v1/jobs", "post", "name"),
        ("/api/v1/jobs", "post", "category"),
        ("/api/v1/jobs", "post", "priority"),
        ("/api/v1/jobs/{id}", "delete", "delete_files"),
    ] {
        let params = doc["paths"][path][method]["parameters"].as_array().unwrap();
        let param = params
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(param["in"], "query", "{method} {path} {name}");
        assert_ne!(param["required"], true, "{name} is optional");
    }
}
