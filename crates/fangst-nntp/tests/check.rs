use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_nntp::{CheckError, ServerConfig, check_server, tls_config_with_roots};

fn config(port: u16, connections: usize) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".to_owned(),
        port,
        tls: None,
        credentials: None,
        max_connections: connections,
        connect_timeout: Duration::from_secs(5),
        command_timeout: Duration::from_secs(5),
    }
}

async fn server(config: Config) -> MockServer {
    MockServer::start(Library::new(), config).await.unwrap()
}

#[tokio::test]
async fn healthy_server_accepts_every_connection() {
    let s = server(Config {
        max_connections: 8,
        ..Config::default()
    })
    .await;
    let check = check_server(&config(s.addr.port(), 8)).await.unwrap();
    assert_eq!(
        (check.connections_accepted, check.connections_requested),
        (8, 8)
    );
}

#[tokio::test]
async fn too_many_connections_names_the_real_limit() {
    let s = server(Config {
        max_connections: 3,
        ..Config::default()
    })
    .await;
    let err = check_server(&config(s.addr.port(), 5)).await.unwrap_err();
    assert_eq!(
        err,
        CheckError::ConnectionLimit {
            accepted: 3,
            requested: 5
        }
    );
    assert_eq!(err.cause(), "connection_limit");
    assert!(
        err.to_string().contains("accepted 3 of 5 connections"),
        "{err}"
    );
}

#[tokio::test]
async fn wrong_password_is_auth_failed_and_hides_the_password() {
    let creds = Some(("user".to_owned(), "right".to_owned()));
    let s = server(Config {
        credentials: creds,
        ..Config::default()
    })
    .await;
    let cfg = ServerConfig {
        credentials: Some(("user".to_owned(), "hunter2-secret".to_owned())),
        ..config(s.addr.port(), 1)
    };
    let err = check_server(&cfg).await.unwrap_err();
    assert_eq!(err.cause(), "auth_failed");
    assert!(!err.to_string().contains("hunter2-secret"));
    assert!(!format!("{err:?}").contains("hunter2-secret"));
}

#[tokio::test]
async fn closed_port_is_refused() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    assert_eq!(
        check_server(&config(port, 1)).await.unwrap_err().cause(),
        "refused"
    );
}

#[tokio::test]
async fn unknown_host_is_dns() {
    let cfg = ServerConfig {
        host: "fangst-check.invalid".to_owned(),
        ..config(119, 1)
    };
    assert_eq!(check_server(&cfg).await.unwrap_err().cause(), "dns");
}

#[tokio::test]
async fn untrusted_certificate_is_tls() {
    let s = server(Config {
        tls: true,
        ..Config::default()
    })
    .await;
    let cfg = ServerConfig {
        host: "localhost".to_owned(),
        tls: Some(tls_config_with_roots(rustls::RootCertStore::empty()).unwrap()),
        ..config(s.addr.port(), 1)
    };
    assert_eq!(check_server(&cfg).await.unwrap_err().cause(), "tls");
}

#[tokio::test]
async fn silent_server_is_timeout() {
    let s = server(Config {
        latency: Duration::from_millis(300),
        ..Config::default()
    })
    .await;
    let cfg = ServerConfig {
        credentials: Some(("u".into(), "p".into())),
        command_timeout: Duration::from_millis(50),
        ..config(s.addr.port(), 1)
    };
    assert_eq!(check_server(&cfg).await.unwrap_err().cause(), "timeout");
}
