//! A mock NNTP server for tests and benchmarks.
//!
//! It serves yEnc articles generated from in-memory files and can be told to behave like a real
//! provider: per-command latency, a connection cap, required credentials, TLS and missing articles.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Articles the server can serve, plus the NZB that describes them.
#[derive(Debug, Default, Clone)]
pub struct Library {
    articles: HashMap<String, Arc<Vec<u8>>>,
    files: Vec<FileEntry>,
}

#[derive(Debug, Clone)]
struct FileEntry {
    name: String,
    group: String,
    segments: Vec<(u32, u64, String)>,
}

impl Library {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Splits `data` into `segment_size` parts, yEnc-encodes each one and returns their message ids.
    ///
    /// # Panics
    /// When `segment_size` is zero.
    pub fn add_file(
        &mut self,
        name: &str,
        data: &[u8],
        segment_size: usize,
        group: &str,
    ) -> Vec<String> {
        assert!(segment_size > 0, "segment size must be positive");
        let file_index = self.files.len();
        let total = u32::try_from(data.len().div_ceil(segment_size).max(1))
            .expect("part count fits in u32");
        let mut segments = Vec::new();
        for (i, chunk) in data.chunks(segment_size).enumerate() {
            let number = u32::try_from(i + 1).expect("part number fits in u32");
            let begin = (i * segment_size) as u64 + 1;
            let end = begin + chunk.len() as u64 - 1;
            let id = format!("f{file_index}.p{number}@fangst-bench.invalid");
            let article = fangst_yenc::encode_part(data, begin, end, number, total, name, 128);
            self.articles.insert(id.clone(), Arc::new(article));
            segments.push((number, chunk.len() as u64, id));
        }
        let ids = segments.iter().map(|(_, _, id)| id.clone()).collect();
        self.files.push(FileEntry {
            name: name.to_owned(),
            group: group.to_owned(),
            segments,
        });
        ids
    }

    /// Stores an article body as given, for edge cases a well-behaved encoder never produces,
    /// such as lines starting with '.'. Raw articles are not listed in the NZB.
    pub fn add_raw_article(&mut self, message_id: &str, body: &[u8]) {
        self.articles
            .insert(message_id.to_owned(), Arc::new(body.to_vec()));
    }

    /// The NZB document listing every file added so far.
    #[must_use]
    pub fn nzb(&self) -> String {
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<nzb xmlns=\"http://www.newzbin.com/DTD/2003/nzb\">\n",
        );
        for (i, file) in self.files.iter().enumerate() {
            let subject = format!(
                "[{}/{}] - \"{}\" yEnc (1/{})",
                i + 1,
                self.files.len(),
                file.name,
                file.segments.len()
            );
            let _ = writeln!(
                xml,
                "  <file poster=\"fangst-bench\" date=\"1700000000\" subject=\"{}\">\n    <groups><group>{}</group></groups>\n    <segments>",
                escape(&subject),
                escape(&file.group)
            );
            for (number, bytes, id) in &file.segments {
                let _ = writeln!(
                    xml,
                    "      <segment bytes=\"{bytes}\" number=\"{number}\">{}</segment>",
                    escape(id)
                );
            }
            xml.push_str("    </segments>\n  </file>\n");
        }
        xml.push_str("</nzb>\n");
        xml
    }

    #[must_use]
    pub fn message_ids(&self) -> Vec<String> {
        self.files
            .iter()
            .flat_map(|f| f.segments.iter().map(|(_, _, id)| id.clone()))
            .collect()
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Delay before every response, to mimic the round trip to a real provider.
    pub latency: Duration,
    /// Connections beyond this get `502` and are closed, like a provider enforcing its plan.
    pub max_connections: usize,
    pub credentials: Option<(String, String)>,
    /// Message ids the server answers with `430`.
    pub missing: HashSet<String>,
    pub tls: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            latency: Duration::ZERO,
            max_connections: 50,
            credentials: None,
            missing: HashSet::new(),
            tls: false,
        }
    }
}

#[derive(Debug, Default)]
struct Stats {
    connections: AtomicU64,
    refused: AtomicU64,
    bodies: AtomicU64,
    missing: AtomicU64,
    bytes: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub connections: u64,
    pub refused: u64,
    pub bodies: u64,
    pub missing: u64,
    pub bytes: u64,
}

pub struct MockServer {
    pub addr: SocketAddr,
    /// DER certificate to trust when `Config::tls` is on.
    pub cert_der: Option<Vec<u8>>,
    stats: Arc<Stats>,
    task: JoinHandle<()>,
}

impl MockServer {
    /// Binds to a free port on localhost and starts serving.
    ///
    /// # Errors
    /// When binding fails or the TLS certificate can't be generated.
    pub async fn start(library: Library, config: Config) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (acceptor, cert_der) = if config.tls {
            let (acceptor, der) = tls_acceptor().map_err(std::io::Error::other)?;
            (Some(acceptor), Some(der))
        } else {
            (None, None)
        };
        let shared = Arc::new(Shared {
            library,
            config,
            stats: Arc::new(Stats::default()),
            active: AtomicUsize::new(0),
        });
        let stats = Arc::clone(&shared.stats);
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let shared = Arc::clone(&shared);
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let _ = stream.set_nodelay(true);
                    match acceptor {
                        Some(acceptor) => {
                            if let Ok(tls) = acceptor.accept(stream).await {
                                serve(tls, &shared).await;
                            }
                        }
                        None => serve(stream, &shared).await,
                    }
                });
            }
        });
        Ok(Self {
            addr,
            cert_der,
            stats,
            task,
        })
    }

    #[must_use]
    pub fn stats(&self) -> StatsSnapshot {
        let s = &self.stats;
        StatsSnapshot {
            connections: s.connections.load(Ordering::Relaxed),
            refused: s.refused.load(Ordering::Relaxed),
            bodies: s.bodies.load(Ordering::Relaxed),
            missing: s.missing.load(Ordering::Relaxed),
            bytes: s.bytes.load(Ordering::Relaxed),
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Shared {
    library: Library,
    config: Config,
    stats: Arc<Stats>,
    active: AtomicUsize,
}

struct ActiveGuard<'a>(&'a AtomicUsize);

impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn serve<S: AsyncRead + AsyncWrite + Unpin>(stream: S, shared: &Shared) {
    let (read, mut write) = tokio::io::split(stream);
    if shared.active.fetch_add(1, Ordering::SeqCst) >= shared.config.max_connections {
        shared.active.fetch_sub(1, Ordering::SeqCst);
        shared.stats.refused.fetch_add(1, Ordering::Relaxed);
        let _ = write.write_all(b"502 too many connections\r\n").await;
        let _ = write.shutdown().await;
        return;
    }
    let _guard = ActiveGuard(&shared.active);
    shared.stats.connections.fetch_add(1, Ordering::Relaxed);

    let mut reader = BufReader::new(read);
    let mut session = Session {
        authed: shared.config.credentials.is_none(),
        user: None,
    };
    if write
        .write_all(b"200 fangst-bench ready\r\n")
        .await
        .is_err()
    {
        return;
    }
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if !shared.config.latency.is_zero() {
            tokio::time::sleep(shared.config.latency).await;
        }
        let (response, quit) = respond(line.trim_end(), &mut session, shared);
        if write.write_all(&response).await.is_err() || quit {
            let _ = write.shutdown().await;
            return;
        }
    }
}

struct Session {
    authed: bool,
    user: Option<String>,
}

fn respond(line: &str, session: &mut Session, shared: &Shared) -> (Vec<u8>, bool) {
    let (command, arg) = line.split_once(' ').unwrap_or((line, ""));
    let command = command.to_ascii_uppercase();
    let reply = |text: &str| (format!("{text}\r\n").into_bytes(), false);
    match command.as_str() {
        "QUIT" => (b"205 bye\r\n".to_vec(), true),
        "CAPABILITIES" => reply("101 capabilities\r\nVERSION 2\r\nREADER\r\nAUTHINFO USER\r\n."),
        "MODE" => reply("200 reader mode"),
        "AUTHINFO" => {
            let (kind, value) = arg.split_once(' ').unwrap_or((arg, ""));
            match (
                kind.to_ascii_uppercase().as_str(),
                &shared.config.credentials,
            ) {
                ("USER", _) => {
                    session.user = Some(value.to_owned());
                    reply("381 password required")
                }
                ("PASS", Some((user, pass)))
                    if session.user.as_deref() == Some(user) && value == pass =>
                {
                    session.authed = true;
                    reply("281 authentication accepted")
                }
                ("PASS", None) => {
                    session.authed = true;
                    reply("281 authentication accepted")
                }
                _ => reply("481 authentication failed"),
            }
        }
        _ if !session.authed => reply("480 authentication required"),
        "GROUP" => reply(&format!("211 0 0 0 {arg}")),
        "BODY" | "ARTICLE" | "STAT" => article(&command, arg, shared),
        _ => reply("500 command not recognized"),
    }
}

fn article(command: &str, arg: &str, shared: &Shared) -> (Vec<u8>, bool) {
    let id = arg.trim().trim_start_matches('<').trim_end_matches('>');
    let found = (!shared.config.missing.contains(id))
        .then(|| shared.library.articles.get(id))
        .flatten();
    let Some(body) = found else {
        shared.stats.missing.fetch_add(1, Ordering::Relaxed);
        return (b"430 no such article\r\n".to_vec(), false);
    };
    let (code, with_headers) = match command {
        "STAT" => return (format!("223 0 <{id}>\r\n").into_bytes(), false),
        "ARTICLE" => (220, true),
        _ => (222, false),
    };
    let mut out = Vec::with_capacity(body.len() + 128);
    out.extend(format!("{code} 0 <{id}>\r\n").into_bytes());
    if with_headers {
        out.extend(format!("Message-ID: <{id}>\r\nSubject: fangst-bench\r\n\r\n").into_bytes());
    }
    for line in body.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        // NNTP dot-stuffing: a line starting with '.' gets a second one so it can't end the response.
        if line.first() == Some(&b'.') {
            out.push(b'.');
        }
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b".\r\n");
    shared.stats.bodies.fetch_add(1, Ordering::Relaxed);
    shared
        .stats
        .bytes
        .fetch_add(out.len() as u64, Ordering::Relaxed);
    (out, false)
}

fn tls_acceptor() -> Result<(tokio_rustls::TlsAcceptor, Vec<u8>), String> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
        .map_err(|e| e.to_string())?;
    let der = cert.cert.der().to_vec();
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(cert.signing_key.serialize_der().into());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_no_client_auth()
    .with_single_cert(
        vec![rustls::pki_types::CertificateDer::from(der.clone())],
        key,
    )
    .map_err(|e| e.to_string())?;
    Ok((tokio_rustls::TlsAcceptor::from(Arc::new(config)), der))
}
