use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fangst_bench::{Config, Library, MockServer};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

struct Client<S> {
    reader: BufReader<S>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Client<S> {
    async fn new(stream: S) -> (Self, String) {
        let mut client = Self {
            reader: BufReader::new(stream),
        };
        let greeting = client.line().await;
        (client, greeting)
    }

    async fn line(&mut self) -> String {
        let mut line = String::new();
        self.reader.read_line(&mut line).await.unwrap();
        line.trim_end().to_owned()
    }

    async fn send(&mut self, command: &str) {
        self.reader
            .get_mut()
            .write_all(format!("{command}\r\n").as_bytes())
            .await
            .unwrap();
    }

    async fn command(&mut self, command: &str) -> String {
        self.send(command).await;
        self.line().await
    }

    /// Reads a multi-line block up to the terminating "." and undoes dot-stuffing.
    async fn block(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut line = Vec::new();
            self.reader.read_until(b'\n', &mut line).await.unwrap();
            if line == b".\r\n" {
                return out;
            }
            let unstuffed = if line.starts_with(b"..") {
                &line[1..]
            } else {
                &line[..]
            };
            out.extend_from_slice(unstuffed);
        }
    }
}

fn sample() -> (Library, Vec<u8>, Vec<String>) {
    let data: Vec<u8> = (0..20_000u32)
        .map(|i| (i * 7 % 251) as u8)
        .chain(std::iter::repeat_n(4u8, 300))
        .collect();
    let mut library = Library::new();
    let ids = library.add_file("sample.bin", &data, 4096, "alt.binaries.test");
    (library, data, ids)
}

async fn plain(server: &MockServer) -> (Client<TcpStream>, String) {
    Client::new(TcpStream::connect(server.addr).await.unwrap()).await
}

#[tokio::test]
async fn serves_bodies_that_decode_back_to_the_file() {
    let (library, data, ids) = sample();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let (mut client, greeting) = plain(&server).await;
    assert!(greeting.starts_with("200"));

    let mut joined = Vec::new();
    for id in &ids {
        assert!(
            client
                .command(&format!("BODY <{id}>"))
                .await
                .starts_with("222")
        );
        joined.extend(fangst_yenc::decode(&client.block().await).unwrap().data);
    }
    assert_eq!(joined, data);
    assert_eq!(server.stats().bodies, ids.len() as u64);
}

#[tokio::test]
async fn nzb_lists_every_segment() {
    let (library, data, ids) = sample();
    let nzb = fangst_nzb::parse(library.nzb().as_bytes(), &fangst_nzb::Limits::default()).unwrap();
    let file = &nzb.files[0];
    assert_eq!(file.file_name().as_deref(), Some("sample.bin"));
    assert_eq!(file.groups, ["alt.binaries.test"]);
    assert_eq!(file.total_bytes(), data.len() as u64);
    let listed: Vec<&str> = file
        .segments
        .iter()
        .map(|s| s.message_id.as_str())
        .collect();
    assert_eq!(listed, ids.iter().map(String::as_str).collect::<Vec<_>>());
}

#[tokio::test]
async fn pipelined_commands_are_answered_in_order() {
    let (library, _, ids) = sample();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let (mut client, _) = plain(&server).await;
    let batch = ids.iter().take(3).fold(String::new(), |mut batch, id| {
        use std::fmt::Write as _;
        let _ = write!(batch, "BODY <{id}>\r\n");
        batch
    });
    client
        .reader
        .get_mut()
        .write_all(batch.as_bytes())
        .await
        .unwrap();
    for id in ids.iter().take(3) {
        assert_eq!(client.line().await, format!("222 0 <{id}>"));
        fangst_yenc::decode(&client.block().await).unwrap();
    }
}

#[tokio::test]
async fn lines_starting_with_a_dot_are_stuffed() {
    let mut library = Library::new();
    let body = b".starts with a dot\r\n..two dots\r\nplain\r\n.\r\n";
    library.add_raw_article("dots@x", body);
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let (mut client, _) = plain(&server).await;
    client.send("BODY <dots@x>").await;
    assert!(client.line().await.starts_with("222"));
    let mut raw = Vec::new();
    loop {
        let mut line = Vec::new();
        client.reader.read_until(b'\n', &mut line).await.unwrap();
        let end = line == b".\r\n";
        raw.extend(line);
        if end {
            break;
        }
    }
    assert_eq!(
        raw,
        b"..starts with a dot\r\n...two dots\r\nplain\r\n..\r\n.\r\n"
    );
}

#[tokio::test]
async fn missing_articles_get_430() {
    let (library, _, ids) = sample();
    let config = Config {
        missing: HashSet::from([ids[1].clone()]),
        ..Config::default()
    };
    let server = MockServer::start(library, config).await.unwrap();
    let (mut client, _) = plain(&server).await;
    assert!(
        client
            .command(&format!("BODY <{}>", ids[1]))
            .await
            .starts_with("430")
    );
    assert!(
        client
            .command("BODY <never@posted>")
            .await
            .starts_with("430")
    );
    assert!(
        client
            .command(&format!("STAT <{}>", ids[0]))
            .await
            .starts_with("223")
    );
    assert_eq!(server.stats().missing, 2);
}

#[tokio::test]
async fn credentials_are_required_when_configured() {
    let (library, _, ids) = sample();
    let config = Config {
        credentials: Some(("user".into(), "pass".into())),
        ..Config::default()
    };
    let server = MockServer::start(library, config).await.unwrap();
    let (mut client, _) = plain(&server).await;
    assert!(
        client
            .command(&format!("BODY <{}>", ids[0]))
            .await
            .starts_with("480")
    );
    assert!(
        client
            .command("AUTHINFO USER user")
            .await
            .starts_with("381")
    );
    assert!(
        client
            .command("AUTHINFO PASS wrong")
            .await
            .starts_with("481")
    );
    assert!(
        client
            .command("AUTHINFO USER user")
            .await
            .starts_with("381")
    );
    assert!(
        client
            .command("AUTHINFO PASS pass")
            .await
            .starts_with("281")
    );
    assert!(
        client
            .command(&format!("BODY <{}>", ids[0]))
            .await
            .starts_with("222")
    );
}

#[tokio::test]
async fn connections_over_the_limit_are_refused() {
    let (library, _, _) = sample();
    let config = Config {
        max_connections: 2,
        ..Config::default()
    };
    let server = MockServer::start(library, config).await.unwrap();
    let (first, ga) = plain(&server).await;
    let (_b, gb) = plain(&server).await;
    let (_c, gc) = plain(&server).await;
    assert!(ga.starts_with("200") && gb.starts_with("200"));
    assert!(gc.starts_with("502"), "{gc}");
    assert_eq!(server.stats().refused, 1);

    drop(first);
    // The freed slot is released when the server notices the close.
    let mut accepted = false;
    for _ in 0..50 {
        let (_d, gd) = plain(&server).await;
        if gd.starts_with("200") {
            accepted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(accepted);
}

#[tokio::test]
async fn latency_is_applied_per_command() {
    let (library, _, _) = sample();
    let config = Config {
        latency: Duration::from_millis(40),
        ..Config::default()
    };
    let server = MockServer::start(library, config).await.unwrap();
    let (mut client, _) = plain(&server).await;
    let started = Instant::now();
    client.command("MODE READER").await;
    client.command("MODE READER").await;
    assert!(started.elapsed() >= Duration::from_millis(80));
}

#[tokio::test]
async fn tls_works_with_the_generated_certificate() {
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
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let tcp = TcpStream::connect(server.addr).await.unwrap();
    let tls = connector
        .connect("localhost".try_into().unwrap(), tcp)
        .await
        .unwrap();
    let (mut client, greeting) = Client::new(tls).await;
    assert!(greeting.starts_with("200"));
    assert!(
        client
            .command(&format!("BODY <{}>", ids[0]))
            .await
            .starts_with("222")
    );
    assert_eq!(
        fangst_yenc::decode(&client.block().await).unwrap().data,
        &data[..4096]
    );
}
