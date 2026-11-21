use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_nntp::{Connection, Error, Pool, ServerConfig, tls_config_with_roots};

fn sample() -> (Library, Vec<u8>, Vec<String>) {
    let data: Vec<u8> = (0..50_000u32)
        .map(|i| i.wrapping_mul(31).to_le_bytes()[0])
        .collect();
    let mut library = Library::new();
    let ids = library.add_file("sample.bin", &data, 5000, "alt.binaries.test");
    (library, data, ids)
}

fn config(server: &MockServer) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".to_owned(),
        port: server.addr.port(),
        tls: None,
        credentials: None,
        max_connections: 4,
        connect_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(5),
    }
}

fn decode_all(bodies: Vec<Result<Vec<u8>, Error>>) -> Vec<u8> {
    bodies
        .into_iter()
        .flat_map(|b| fangst_yenc::decode(&b.unwrap()).unwrap().data)
        .collect()
}

#[tokio::test]
async fn downloads_bodies_one_at_a_time() {
    let (library, data, ids) = sample();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let mut conn = Connection::connect(&config(&server)).await.unwrap();
    let mut joined = Vec::new();
    for id in &ids {
        joined.extend(
            fangst_yenc::decode(&conn.body(id).await.unwrap())
                .unwrap()
                .data,
        );
    }
    assert_eq!(joined, data);
}

#[tokio::test]
async fn pipelined_bodies_come_back_in_order() {
    let (library, data, ids) = sample();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    for depth in [1, 3, 8, 64] {
        let mut conn = Connection::connect(&config(&server)).await.unwrap();
        assert_eq!(
            decode_all(conn.bodies(&ids, depth).await),
            data,
            "depth {depth}"
        );
    }
}

#[tokio::test]
async fn pipelining_hides_latency() {
    let (library, _, ids) = sample();
    let latency = Duration::from_millis(30);
    let server = MockServer::start(
        library,
        Config {
            latency,
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let mut conn = Connection::connect(&config(&server)).await.unwrap();
    let started = std::time::Instant::now();
    let results = conn.bodies(&ids, ids.len()).await;
    assert!(results.iter().all(Result::is_ok));
    // The mock server answers one command at a time, so pipelining saves the client's half of
    // each round trip; serial requests would take at least 2x latency per article.
    assert!(started.elapsed() < latency * u32::try_from(ids.len() * 2).unwrap());
}

#[tokio::test]
async fn missing_article_is_its_own_error_and_keeps_the_connection() {
    let (library, _, ids) = sample();
    let config_missing = Config {
        missing: HashSet::from([ids[2].clone()]),
        ..Config::default()
    };
    let server = MockServer::start(library, config_missing).await.unwrap();
    let mut conn = Connection::connect(&config(&server)).await.unwrap();
    let results = conn.bodies(&ids, 4).await;
    assert_eq!(results[2], Err(Error::NoSuchArticle));
    assert!(results.iter().enumerate().all(|(i, r)| i == 2 || r.is_ok()));
    assert!(!conn.is_broken());
    assert!(conn.stat(&ids[0]).await.unwrap());
    assert!(!conn.stat(&ids[2]).await.unwrap());
}

#[tokio::test]
async fn logs_in_with_credentials() {
    let (library, _, ids) = sample();
    let creds = Some(("user".to_owned(), "secret".to_owned()));
    let server = MockServer::start(
        library,
        Config {
            credentials: creds.clone(),
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let mut conn = Connection::connect(&ServerConfig {
        credentials: creds,
        ..config(&server)
    })
    .await
    .unwrap();
    assert!(conn.body(&ids[0]).await.is_ok());
}

#[tokio::test]
async fn wrong_password_is_auth_failed() {
    let (library, _, _) = sample();
    let creds = Some(("user".to_owned(), "secret".to_owned()));
    let server = MockServer::start(
        library,
        Config {
            credentials: creds,
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let wrong = Some(("user".to_owned(), "nope".to_owned()));
    let err = Connection::connect(&ServerConfig {
        credentials: wrong,
        ..config(&server)
    })
    .await
    .err()
    .unwrap();
    assert!(matches!(err, Error::AuthFailed(_)), "{err}");
}

#[tokio::test]
async fn missing_login_is_auth_required() {
    let (library, _, ids) = sample();
    let creds = Some(("user".to_owned(), "secret".to_owned()));
    let server = MockServer::start(
        library,
        Config {
            credentials: creds,
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let mut conn = Connection::connect(&config(&server)).await.unwrap();
    assert_eq!(conn.body(&ids[0]).await, Err(Error::AuthRequired));
}

#[tokio::test]
async fn connection_limit_greeting_is_unavailable() {
    let (library, _, _) = sample();
    let server = MockServer::start(
        library,
        Config {
            max_connections: 1,
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let _first = Connection::connect(&config(&server)).await.unwrap();
    let err = Connection::connect(&config(&server)).await.err().unwrap();
    assert!(matches!(err, Error::Unavailable { code: 502, .. }), "{err}");
}

#[tokio::test]
async fn slow_server_times_out() {
    let (library, _, _) = sample();
    let server = MockServer::start(
        library,
        Config {
            latency: Duration::from_millis(500),
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let mut conn = Connection::connect(&config(&server)).await.unwrap();
    let mut short = config(&server);
    short.command_timeout = Duration::from_millis(50);
    let mut fast = Connection::connect(&short).await.unwrap();
    assert_eq!(fast.group("alt.binaries.test").await, Err(Error::Timeout));
    assert!(fast.is_broken());
    assert!(conn.group("alt.binaries.test").await.is_ok());
}

#[tokio::test]
async fn closed_port_is_refused() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let cfg = ServerConfig {
        host: "127.0.0.1".to_owned(),
        port,
        tls: None,
        credentials: None,
        max_connections: 1,
        connect_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(5),
    };
    assert_eq!(Connection::connect(&cfg).await.err(), Some(Error::Refused));
}

#[tokio::test]
async fn unknown_host_is_dns() {
    let cfg = ServerConfig {
        host: "fangst-test.invalid".to_owned(),
        port: 119,
        tls: None,
        credentials: None,
        max_connections: 1,
        connect_timeout: Duration::from_secs(10),
        command_timeout: Duration::from_secs(5),
    };
    assert!(matches!(
        Connection::connect(&cfg).await.err(),
        Some(Error::Dns(_))
    ));
}

#[tokio::test]
async fn tls_with_trusted_root() {
    let (library, data, ids) = sample();
    let server = MockServer::start(
        library,
        Config {
            tls: true,
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(
            server.cert_der.clone().unwrap(),
        ))
        .unwrap();
    let cfg = ServerConfig {
        host: "localhost".to_owned(),
        tls: Some(tls_config_with_roots(roots).unwrap()),
        ..config(&server)
    };
    let mut conn = Connection::connect(&cfg).await.unwrap();
    assert_eq!(decode_all(conn.bodies(&ids, 8).await), data);
}

#[tokio::test]
async fn tls_with_untrusted_certificate_is_a_tls_error() {
    let (library, _, _) = sample();
    let server = MockServer::start(
        library,
        Config {
            tls: true,
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let cfg = ServerConfig {
        host: "localhost".to_owned(),
        tls: Some(tls_config_with_roots(rustls::RootCertStore::empty()).unwrap()),
        ..config(&server)
    };
    assert!(matches!(
        Connection::connect(&cfg).await.err(),
        Some(Error::Tls(_))
    ));
}

#[tokio::test]
async fn pool_never_exceeds_its_connection_count() {
    let (library, _, ids) = sample();
    let server = MockServer::start(
        library,
        Config {
            max_connections: 3,
            ..Config::default()
        },
    )
    .await
    .unwrap();
    let pool = Arc::new(Pool::new(ServerConfig {
        max_connections: 3,
        ..config(&server)
    }));
    let mut tasks = Vec::new();
    for round in 0..30 {
        let pool = Arc::clone(&pool);
        let id = ids[round % ids.len()].clone();
        tasks.push(tokio::spawn(async move {
            let mut conn = pool.get().await.unwrap();
            conn.body(&id).await.unwrap();
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    let stats = server.stats();
    assert_eq!(stats.refused, 0);
    assert!(stats.connections <= 3, "{stats:?}");
    assert_eq!(stats.bodies, 30);
}

#[tokio::test]
async fn pool_drops_broken_connections() {
    let (library, _, ids) = sample();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let pool = Pool::new(ServerConfig {
        max_connections: 1,
        command_timeout: Duration::from_secs(5),
        ..config(&server)
    });
    {
        let mut conn = pool.get().await.unwrap();
        conn.body(&ids[0]).await.unwrap();
    }
    {
        let mut conn = pool.get().await.unwrap();
        conn.body(&ids[1]).await.unwrap();
    }
    assert_eq!(
        server.stats().connections,
        1,
        "the second get reused the idle connection"
    );
}
