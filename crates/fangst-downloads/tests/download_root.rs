use std::sync::Arc;
use std::time::Duration;

use fangst_downloads::{Runner, Service, Store};
use fangst_nntp::{Pool, ServerConfig};

/// Seen with HolyArrs 1.2.0: it learns where SABnzbd's finished downloads are only when the
/// complete folder exists at the time of its connection test. A fresh install had no download
/// folder until the first job, so the first import waited for a person.
#[tokio::test]
async fn the_download_folder_exists_once_the_service_runs() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let root = dir.path().join("downloads");
    let service = Service::new(Runner {
        store,
        servers: vec![fangst_usenet::Server::primary(Arc::new(Pool::new(
            ServerConfig {
                host: "127.0.0.1".to_owned(),
                port: 9,
                tls: None,
                credentials: None,
                max_connections: 1,
                connect_timeout: Duration::from_secs(1),
                command_timeout: Duration::from_secs(1),
            },
        )))],
        root: root.clone(),
        options: fangst_usenet::Options {
            connections: 1,
            pipeline_depth: 1,
            retries: 1,
        },
        post_processors: Vec::new(),
    });
    let worker = tokio::spawn(async move { service.run().await });
    let appeared = tokio::time::timeout(Duration::from_secs(10), async {
        while !root.is_dir() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    worker.abort();
    assert!(appeared.is_ok(), "{} was not created", root.display());
}
