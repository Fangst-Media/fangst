use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use fangst_core::secret::MasterKey;
use fangst_dl_api::{AuthSettings, LoginLimits, UserDto, router};
use fangst_downloads::{Auth, Runner, Service, Store};
use http_body_util::BodyExt;
use tower::ServiceExt;

// Placeholders only; never real credentials.
const KEY: &str = "environment-key-placeholder";
const PASSWORD: &str = "placeholder-password";

struct Harness {
    app: axum::Router,
    _dir: tempfile::TempDir,
}

async fn harness() -> Harness {
    harness_with_hash_slots(4).await
}

async fn harness_with_hash_slots(slots: usize) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let auth = Auth::with_hash_slots(
        &store,
        Arc::new(MasterKey::load_or_create(&dir.path().join("master.key")).unwrap()),
        slots,
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
        post_processors: Vec::new(),
    };
    let settings = AuthSettings {
        login: LoginLimits {
            free_attempts: 3,
            // Long enough that a slow CI runner never sees the wait run out between two requests.
            base_delay: std::time::Duration::from_mins(1),
            ..LoginLimits::default()
        },
        ..AuthSettings::default()
    };
    Harness {
        app: router(Service::new(runner), auth, settings),
        _dir: dir,
    }
}

struct Reply {
    status: StatusCode,
    set_cookie: Option<String>,
    retry_after: Option<String>,
    body: serde_json::Value,
}

#[derive(Default)]
struct Call<'a> {
    key: Option<&'a str>,
    cookie: Option<&'a str>,
    json: Option<serde_json::Value>,
    from: Option<&'a str>,
    headers: Vec<(&'static str, &'a str)>,
}

/// What a browser adds to a request from the daemon's own pages.
const SAME_ORIGIN: (&str, &str) = ("sec-fetch-site", "same-origin");

async fn send(h: &Harness, method: &str, uri: &str, call: Call<'_>) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(key) = call.key {
        request = request.header("x-api-key", key);
    }
    if let Some(cookie) = call.cookie {
        request = request.header(header::COOKIE, cookie);
    }
    for (name, value) in call.headers {
        request = request.header(name, value);
    }
    let body = match call.json {
        Some(json) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };
    let mut request = request.body(body).unwrap();
    let from: SocketAddr = call.from.unwrap_or("192.0.2.1:5000").parse().unwrap();
    request.extensions_mut().insert(ConnectInfo(from));
    let response = h.app.clone().oneshot(request).await.unwrap();
    let (set_cookie, retry_after) = {
        let text = |name| {
            response
                .headers()
                .get(name)
                .map(|v| v.to_str().unwrap().to_owned())
        };
        (text(header::SET_COOKIE), text(header::RETRY_AFTER))
    };
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        set_cookie,
        retry_after,
        body: serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    }
}

async fn set_first_password(h: &Harness) {
    let reply = send(
        h,
        "PUT",
        "/api/v1/auth/password",
        Call {
            key: Some(KEY),
            json: Some(serde_json::json!({"new_password": PASSWORD})),
            ..Call::default()
        },
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
}

async fn login(h: &Harness, username: &str, password: &str, from: &str) -> Reply {
    send(
        h,
        "POST",
        "/api/v1/auth/login",
        Call {
            json: Some(serde_json::json!({"username": username, "password": password})),
            from: Some(from),
            ..Call::default()
        },
    )
    .await
}

/// `name=value` from a Set-Cookie header, ready for a Cookie header.
fn cookie_pair(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn auth_login_cookie_and_api_key_identify_the_same_user() {
    let h = harness().await;
    set_first_password(&h).await;

    let reply = login(&h, "admin", PASSWORD, "192.0.2.1:1").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let set_cookie = reply.set_cookie.unwrap();
    for attribute in ["HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(set_cookie.contains(attribute), "{set_cookie}");
    }
    assert!(!set_cookie.contains("Secure"), "plain HTTP by default");
    let cookie = cookie_pair(&set_cookie);

    let by_cookie = send(
        &h,
        "GET",
        "/api/v1/auth/me",
        Call {
            cookie: Some(&cookie),
            ..Call::default()
        },
    )
    .await;
    let by_key = send(
        &h,
        "GET",
        "/api/v1/auth/me",
        Call {
            key: Some(KEY),
            ..Call::default()
        },
    )
    .await;
    assert_eq!(by_cookie.status, StatusCode::OK);
    let by_cookie: UserDto = serde_json::from_value(by_cookie.body).unwrap();
    let by_key: UserDto = serde_json::from_value(by_key.body).unwrap();
    assert_eq!(by_cookie, by_key);
    assert_eq!(by_cookie.username, "admin");
    assert!(by_cookie.has_password);

    let jobs = send(
        &h,
        "GET",
        "/api/v1/jobs",
        Call {
            cookie: Some(&format!("theme=dark; {cookie}")),
            ..Call::default()
        },
    )
    .await;
    assert_eq!(
        jobs.status,
        StatusCode::OK,
        "the cookie opens the whole API"
    );

    let out = send(
        &h,
        "POST",
        "/api/v1/auth/logout",
        Call {
            cookie: Some(&cookie),
            ..Call::default()
        },
    )
    .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert!(out.set_cookie.unwrap().contains("Max-Age=0"));
    let after = send(
        &h,
        "GET",
        "/api/v1/auth/me",
        Call {
            cookie: Some(&cookie),
            ..Call::default()
        },
    )
    .await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_failed_login_does_not_reveal_whether_the_user_exists() {
    let h = harness().await;
    set_first_password(&h).await;
    let wrong_password = login(&h, "admin", "not-the-password", "192.0.2.10:1").await;
    let no_such_user = login(&h, "nobody", "not-the-password", "192.0.2.11:1").await;
    assert_eq!(wrong_password.status, StatusCode::UNAUTHORIZED);
    assert_eq!(no_such_user.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong_password.body, no_such_user.body);
    assert!(wrong_password.set_cookie.is_none());

    // The bootstrap user has no password until one is set; that looks the same too.
    let fresh = harness().await;
    let no_password = login(&fresh, "admin", "", "192.0.2.12:1").await;
    assert_eq!(no_password.status, StatusCode::UNAUTHORIZED);
    assert_eq!(no_password.body, no_such_user.body);
}

#[tokio::test]
async fn auth_brute_force_login_is_rate_limited() {
    let h = harness().await;
    set_first_password(&h).await;
    let attacker = "198.51.100.7:4000";
    for _ in 0..3 {
        assert_eq!(
            login(&h, "admin", "guess-guess-guess", attacker)
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
    }
    // The attempt that crosses the limit is still checked; the one after it waits.
    assert_eq!(
        login(&h, "admin", "guess-guess-guess", attacker)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let blocked = login(&h, "admin", PASSWORD, attacker).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    // The wait is counted from the failed attempt, so a slow machine can already be a second in.
    let retry_after: u64 = blocked.retry_after.as_deref().unwrap().parse().unwrap();
    assert!(
        (55..=60).contains(&retry_after),
        "Retry-After {retry_after}"
    );
    assert!(
        blocked.set_cookie.is_none(),
        "even the right password waits"
    );

    let someone_else = login(&h, "admin", PASSWORD, "203.0.113.9:4000").await;
    assert_eq!(someone_else.status, StatusCode::OK);
}

#[tokio::test]
async fn auth_password_change_needs_the_current_password() {
    let h = harness().await;
    set_first_password(&h).await;
    let cookie = cookie_pair(
        &login(&h, "admin", PASSWORD, "192.0.2.1:1")
            .await
            .set_cookie
            .unwrap(),
    );

    let change = |current: Option<&'static str>, new: &'static str| {
        let cookie = cookie.clone();
        let h = &h;
        async move {
            send(
                h,
                "PUT",
                "/api/v1/auth/password",
                Call {
                    cookie: Some(&cookie),
                    json: Some(
                        serde_json::json!({"current_password": current, "new_password": new}),
                    ),
                    headers: vec![SAME_ORIGIN],
                    ..Call::default()
                },
            )
            .await
        }
    };
    assert_eq!(
        change(None, "a-new-placeholder").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        change(Some(PASSWORD), "short").await.status,
        StatusCode::BAD_REQUEST
    );
    let changed = change(Some(PASSWORD), "a-new-placeholder").await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT);
    let fresh = cookie_pair(&changed.set_cookie.expect("a fresh session"));

    let old = send(
        &h,
        "GET",
        "/api/v1/auth/me",
        Call {
            cookie: Some(&cookie),
            ..Call::default()
        },
    )
    .await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED, "old sessions end");
    let new = send(
        &h,
        "GET",
        "/api/v1/auth/me",
        Call {
            cookie: Some(&fresh),
            ..Call::default()
        },
    )
    .await;
    assert_eq!(new.status, StatusCode::OK);
    assert_eq!(
        login(&h, "admin", "a-new-placeholder", "192.0.2.1:1")
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn auth_routes_need_a_caller_except_login_and_logout() {
    let h = harness().await;
    for (method, uri) in [("GET", "/api/v1/auth/me"), ("PUT", "/api/v1/auth/password")] {
        for cookie in [None, Some("fangst_session=forged")] {
            let reply = send(
                &h,
                method,
                uri,
                Call {
                    cookie,
                    ..Call::default()
                },
            )
            .await;
            assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        }
    }
    let out = send(&h, "POST", "/api/v1/auth/logout", Call::default()).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
}

async fn signed_in(h: &Harness) -> String {
    set_first_password(h).await;
    cookie_pair(
        &login(h, "admin", PASSWORD, "192.0.2.1:1")
            .await
            .set_cookie
            .unwrap(),
    )
}

#[tokio::test]
async fn auth_cookie_changes_state_only_from_this_origin() {
    let h = harness().await;
    let cookie = signed_in(&h).await;
    let pause = |headers: Vec<(&'static str, &'static str)>| {
        let cookie = cookie.clone();
        let h = &h;
        async move {
            send(
                h,
                "POST",
                "/api/v1/jobs/99/pause",
                Call {
                    cookie: Some(&cookie),
                    headers,
                    ..Call::default()
                },
            )
            .await
            .status
        }
    };
    // 404 means the request got past the origin check to the (missing) job.
    assert_eq!(pause(vec![SAME_ORIGIN]).await, StatusCode::NOT_FOUND);
    assert_eq!(
        pause(vec![
            ("host", "fangst.lab.example.com"),
            ("origin", "https://fangst.lab.example.com")
        ])
        .await,
        StatusCode::NOT_FOUND
    );

    for headers in [
        vec![
            ("host", "fangst.lab.example.com"),
            ("origin", "https://other.lab.example.com"),
            ("sec-fetch-site", "same-site"),
        ],
        vec![
            ("host", "fangst.lab.example.com:7740"),
            ("origin", "http://fangst.lab.example.com:8989"),
        ],
        vec![("host", "fangst.lab.example.com"), ("origin", "null")],
        vec![("host", "fangst.lab.example.com")],
    ] {
        assert_eq!(
            pause(headers.clone()).await,
            StatusCode::FORBIDDEN,
            "{headers:?}"
        );
    }

    let read = send(
        &h,
        "GET",
        "/api/v1/jobs",
        Call {
            cookie: Some(&cookie),
            headers: vec![("origin", "https://other.lab.example.com")],
            ..Call::default()
        },
    )
    .await;
    assert_eq!(read.status, StatusCode::OK, "reads are not state changes");
}

#[tokio::test]
async fn auth_api_key_needs_no_origin() {
    let h = harness().await;
    for headers in [vec![], vec![("origin", "https://other.lab.example.com")]] {
        let reply = send(
            &h,
            "POST",
            "/api/v1/jobs/99/pause",
            Call {
                key: Some(KEY),
                headers,
                ..Call::default()
            },
        )
        .await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_event_stream_refuses_a_cookie_from_another_origin() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let h = harness().await;
    let cookie = signed_in(&h).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = h.app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    let connect = |origin: String| {
        let cookie = cookie.clone();
        async move {
            let mut request = format!("ws://{addr}/api/v1/events")
                .into_client_request()
                .unwrap();
            let headers = request.headers_mut();
            headers.insert("cookie", cookie.parse().unwrap());
            headers.insert("origin", origin.parse().unwrap());
            tokio_tungstenite::connect_async(request).await
        }
    };
    assert!(
        connect(format!("http://{addr}")).await.is_ok(),
        "the daemon's own page opens the stream"
    );
    match connect("http://127.0.0.1:1".to_owned()).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        other => panic!("expected a refused handshake, got {:?}", other.map(|_| ())),
    }
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auth_logins_past_the_hashing_limit_get_503() {
    let h = harness_with_hash_slots(1).await;
    set_first_password(&h).await;
    let h = Arc::new(h);
    let attempts: Vec<_> = (0..8u8)
        .map(|i| {
            let h = Arc::clone(&h);
            tokio::spawn(
                async move { login(&h, "admin", PASSWORD, &format!("198.51.100.{i}:1")).await },
            )
        })
        .collect();
    let mut busy = 0;
    for attempt in attempts {
        let reply = attempt.await.unwrap();
        if reply.status == StatusCode::SERVICE_UNAVAILABLE {
            assert_eq!(reply.retry_after.as_deref(), Some("1"));
            busy += 1;
        } else {
            assert_eq!(reply.status, StatusCode::OK);
        }
    }
    assert!(busy >= 1, "with one slot, parallel logins are refused");
}
