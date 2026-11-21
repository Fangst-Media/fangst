use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_nntp::{Pool, ServerConfig};
use fangst_usenet::{Options, Progress, Resume, Server, download_from};

fn data(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| {
            u32::try_from(i)
                .unwrap()
                .wrapping_mul(2_654_435_761)
                .to_le_bytes()[2]
        })
        .collect()
}

fn server(mock: &MockServer, name: &str, backfill: bool) -> Server {
    Server {
        name: name.to_owned(),
        pool: Arc::new(Pool::new(ServerConfig {
            host: "127.0.0.1".to_owned(),
            port: mock.addr.port(),
            tls: None,
            credentials: None,
            max_connections: 2,
            connect_timeout: Duration::from_secs(5),
            command_timeout: Duration::from_secs(5),
        })),
        backfill,
    }
}

const OPTIONS: Options = Options {
    connections: 2,
    pipeline_depth: 4,
    retries: 1,
};

fn quiet() -> fangst_usenet::ProgressFn {
    Arc::new(|_: &Progress| {})
}

struct Setup {
    library: Library,
    file: Vec<u8>,
    ids: Vec<String>,
    nzb: fangst_nzb::Nzb,
}

fn setup() -> Setup {
    let file = data(100_000);
    let mut library = Library::new();
    let ids = library.add_file("multi.bin", &file, 5000, "g");
    let nzb = fangst_nzb::parse(library.nzb().as_bytes(), &fangst_nzb::Limits::default()).unwrap();
    Setup {
        library,
        file,
        ids,
        nzb,
    }
}

async fn mock(library: &Library, missing: &[&String]) -> MockServer {
    let config = Config {
        missing: missing
            .iter()
            .map(|id| (*id).clone())
            .collect::<HashSet<_>>(),
        ..Config::default()
    };
    MockServer::start(library.clone(), config).await.unwrap()
}

#[tokio::test]
async fn backfill_only_serves_what_the_primary_is_missing() {
    let s = setup();
    let primary = mock(&s.library, &[&s.ids[3], &s.ids[11]]).await;
    let backfill = mock(&s.library, &[]).await;
    let out = tempfile::tempdir().unwrap();

    let report = download_from(
        &s.nzb,
        out.path(),
        vec![
            server(&primary, "primary", false),
            server(&backfill, "backfill", true),
        ],
        OPTIONS,
        quiet(),
        Resume::default(),
    )
    .await
    .unwrap();

    assert!(report.is_complete(), "{report:?}");
    assert_eq!(std::fs::read(out.path().join("multi.bin")).unwrap(), s.file);
    assert_eq!(
        backfill.stats().bodies,
        2,
        "backfill fetched only the two missing articles"
    );
    assert_eq!(
        backfill.stats().missing,
        0,
        "backfill was never asked for anything else"
    );
    let total = s.ids.len();
    assert_eq!(
        (report.servers[0].served, report.servers[0].missed),
        (total - 2, 2)
    );
    assert_eq!((report.servers[1].served, report.servers[1].missed), (2, 0));
}

#[tokio::test]
async fn second_primary_fills_gaps_before_any_backfill() {
    let s = setup();
    let a = mock(&s.library, &[&s.ids[0]]).await;
    let b = mock(&s.library, &[&s.ids[1]]).await;
    let backfill = mock(&s.library, &[]).await;
    let out = tempfile::tempdir().unwrap();

    let report = download_from(
        &s.nzb,
        out.path(),
        vec![
            server(&a, "a", false),
            server(&b, "b", false),
            server(&backfill, "backfill", true),
        ],
        OPTIONS,
        quiet(),
        Resume::default(),
    )
    .await
    .unwrap();

    assert!(report.is_complete(), "{report:?}");
    assert_eq!(std::fs::read(out.path().join("multi.bin")).unwrap(), s.file);
    assert_eq!(
        backfill.stats().bodies + backfill.stats().missing,
        0,
        "backfill untouched"
    );
}

#[tokio::test]
async fn missing_everywhere_is_missing() {
    let s = setup();
    let primary = mock(&s.library, &[&s.ids[5]]).await;
    let backfill = mock(&s.library, &[&s.ids[5]]).await;
    let out = tempfile::tempdir().unwrap();

    let report = download_from(
        &s.nzb,
        out.path(),
        vec![
            server(&primary, "primary", false),
            server(&backfill, "backfill", true),
        ],
        OPTIONS,
        quiet(),
        Resume::default(),
    )
    .await
    .unwrap();

    assert_eq!(report.files[0].missing_segments, [6]);
    assert_eq!((report.servers[0].missed, report.servers[1].missed), (1, 1));
}

#[tokio::test]
async fn no_servers_is_an_error() {
    let s = setup();
    let out = tempfile::tempdir().unwrap();
    let result = download_from(
        &s.nzb,
        out.path(),
        Vec::new(),
        OPTIONS,
        quiet(),
        Resume::default(),
    )
    .await;
    assert_eq!(result.err(), Some(fangst_usenet::Error::Servers(0)));
}
