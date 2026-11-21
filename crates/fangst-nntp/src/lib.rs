//! Async NNTP client for downloading binaries.
//!
//! [`Connection`] speaks the reader commands a downloader needs and pipelines `BODY` requests so a
//! high-latency link stays busy. [`Pool`] caps connections per server, because providers enforce a
//! connection count per account and answer extra connections with an error.
//! Errors are typed so callers can tell a missing article from a broken connection or a bad login.

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf, WriteHalf,
};
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// `None` for plain TCP.
    pub tls: Option<Arc<rustls::ClientConfig>>,
    pub credentials: Option<(String, String)>,
    pub max_connections: usize,
    pub connect_timeout: Duration,
    pub command_timeout: Duration,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Credentials stay out of logs.
        f.debug_struct("ServerConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("tls", &self.tls.is_some())
            .field("credentials", &self.credentials.as_ref().map(|_| "<set>"))
            .field("max_connections", &self.max_connections)
            .finish_non_exhaustive()
    }
}

/// TLS settings trusting the operating system's root certificates.
///
/// # Errors
/// When no usable root certificate can be loaded.
pub fn native_tls_config() -> Result<Arc<rustls::ClientConfig>, Error> {
    let mut roots = rustls::RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();
    for cert in loaded.certs {
        let _ = roots.add(cert);
    }
    if roots.is_empty() {
        return Err(Error::Tls(
            "no root certificates found on this system".to_owned(),
        ));
    }
    tls_config_with_roots(roots)
}

/// TLS settings trusting only the given roots, for self-signed servers and tests.
///
/// # Errors
/// When the TLS provider can't be set up.
pub fn tls_config_with_roots(
    roots: rustls::RootCertStore,
) -> Result<Arc<rustls::ClientConfig>, Error> {
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| Error::Tls(e.to_string()))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(Arc::new(config))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The host name did not resolve.
    Dns(String),
    /// Nothing is listening on the port.
    Refused,
    Timeout,
    Tls(String),
    Io(String),
    /// 430: the server does not have this article.
    NoSuchArticle,
    /// 481 or 482: wrong user name or password.
    AuthFailed(String),
    /// 480: the server wants a login and none is configured.
    AuthRequired,
    /// 502 or 400 on connect: the account's connection limit is reached or the service is down.
    Unavailable {
        code: u16,
        message: String,
    },
    /// Any other reply the client did not expect.
    Protocol {
        code: u16,
        message: String,
    },
}

impl Error {
    /// Whether the connection can still be used after this error.
    #[must_use]
    pub fn connection_usable(&self) -> bool {
        matches!(self, Self::NoSuchArticle | Self::Protocol { .. })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dns(host) => write!(f, "could not resolve host {host}"),
            Self::Refused => f.write_str("connection refused; nothing is listening on that port"),
            Self::Timeout => f.write_str("the server did not answer in time"),
            Self::Tls(msg) => write!(f, "TLS failed: {msg}"),
            Self::Io(msg) => write!(f, "network error: {msg}"),
            Self::NoSuchArticle => f.write_str("the server does not have this article (430)"),
            Self::AuthFailed(msg) => write!(f, "login rejected: {msg}"),
            Self::AuthRequired => f.write_str("the server requires a login"),
            Self::Unavailable { code, message } => {
                write!(f, "server unavailable ({code}): {message}")
            }
            Self::Protocol { code, message } => write!(f, "unexpected reply {code}: {message}"),
        }
    }
}

impl std::error::Error for Error {}

fn io_error(e: &std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::ConnectionRefused => Error::Refused,
        std::io::ErrorKind::TimedOut => Error::Timeout,
        _ => Error::Io(e.to_string()),
    }
}

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupInfo {
    pub count: u64,
    pub first: u64,
    pub last: u64,
}

pub struct Connection {
    reader: BufReader<ReadHalf<Box<dyn Stream>>>,
    writer: WriteHalf<Box<dyn Stream>>,
    timeout: Duration,
    line: Vec<u8>,
    broken: bool,
}

impl Connection {
    /// Connects, reads the greeting and logs in when credentials are configured.
    ///
    /// # Errors
    /// Typed by cause: DNS, refused, timeout, TLS, login, or the server turning the connection away.
    pub async fn connect(config: &ServerConfig) -> Result<Self, Error> {
        let address = format!("{}:{}", config.host, config.port);
        let addrs: Vec<_> =
            tokio::time::timeout(config.connect_timeout, tokio::net::lookup_host(address))
                .await
                .map_err(|_| Error::Timeout)?
                .map_err(|_| Error::Dns(config.host.clone()))?
                .collect();
        if addrs.is_empty() {
            return Err(Error::Dns(config.host.clone()));
        }
        // Providers publish IPv4 and IPv6 addresses and either may be unreachable from a given
        // network, so every resolved address is tried before giving up.
        let mut last_error = Error::Refused;
        let mut connected = None;
        for addr in addrs {
            match tokio::time::timeout(config.connect_timeout, TcpStream::connect(addr)).await {
                Ok(Ok(tcp)) => {
                    connected = Some(tcp);
                    break;
                }
                Ok(Err(e)) => last_error = io_error(&e),
                Err(_) => last_error = Error::Timeout,
            }
        }
        let tcp = connected.ok_or(last_error)?;
        let _ = tcp.set_nodelay(true);

        let stream: Box<dyn Stream> = match &config.tls {
            None => Box::new(tcp),
            Some(tls) => {
                let name = rustls::pki_types::ServerName::try_from(config.host.clone())
                    .map_err(|e| Error::Tls(e.to_string()))?;
                let connector = tokio_rustls::TlsConnector::from(Arc::clone(tls));
                let stream =
                    tokio::time::timeout(config.connect_timeout, connector.connect(name, tcp))
                        .await
                        .map_err(|_| Error::Timeout)?
                        .map_err(|e| Error::Tls(e.to_string()))?;
                Box::new(stream)
            }
        };
        let (read, write) = tokio::io::split(stream);
        let mut conn = Self {
            reader: BufReader::with_capacity(256 * 1024, read),
            writer: write,
            timeout: config.command_timeout,
            line: Vec::with_capacity(1024),
            broken: false,
        };

        let (code, message) = conn.status().await?;
        match code {
            200 | 201 => {}
            400 | 502 => return Err(Error::Unavailable { code, message }),
            _ => return Err(Error::Protocol { code, message }),
        }
        if let Some((user, pass)) = &config.credentials {
            conn.login(user, pass).await?;
        }
        Ok(conn)
    }

    async fn login(&mut self, user: &str, pass: &str) -> Result<(), Error> {
        let (code, message) = self.command(&format!("AUTHINFO USER {user}")).await?;
        let (code, message) = match code {
            281 => return Ok(()),
            381 => self.command(&format!("AUTHINFO PASS {pass}")).await?,
            _ => (code, message),
        };
        match code {
            281 => Ok(()),
            481 | 482 | 502 => Err(Error::AuthFailed(message)),
            _ => Err(Error::Protocol { code, message }),
        }
    }

    /// Selects a newsgroup. Most providers serve articles by message id without one.
    ///
    /// # Errors
    /// When the group doesn't exist or the connection fails.
    pub async fn group(&mut self, name: &str) -> Result<GroupInfo, Error> {
        let (code, message) = self.command(&format!("GROUP {name}")).await?;
        if code != 211 {
            return Err(self.reply_error(code, message));
        }
        let mut numbers = message
            .split_whitespace()
            .map(|n| n.parse::<u64>().unwrap_or(0));
        Ok(GroupInfo {
            count: numbers.next().unwrap_or(0),
            first: numbers.next().unwrap_or(0),
            last: numbers.next().unwrap_or(0),
        })
    }

    /// Whether the server has the article, without downloading it.
    ///
    /// # Errors
    /// When the connection fails or the server answers something other than 223 or 430.
    pub async fn stat(&mut self, message_id: &str) -> Result<bool, Error> {
        let (code, message) = self
            .command(&format!("STAT <{}>", bare(message_id)))
            .await?;
        match code {
            223 => Ok(true),
            430 => Ok(false),
            _ => Err(self.reply_error(code, message)),
        }
    }

    /// Downloads one article body with dot-stuffing undone.
    ///
    /// # Errors
    /// [`Error::NoSuchArticle`] for 430; other variants mean the connection is unusable.
    pub async fn body(&mut self, message_id: &str) -> Result<Vec<u8>, Error> {
        self.bodies(&[message_id], 1)
            .await
            .pop()
            .unwrap_or(Err(Error::Io("no response".to_owned())))
    }

    /// Downloads bodies with up to `depth` requests in flight, returning results in request order.
    ///
    /// Once the connection breaks, every remaining request gets the same error.
    pub async fn bodies<S: AsRef<str>>(
        &mut self,
        message_ids: &[S],
        depth: usize,
    ) -> Vec<Result<Vec<u8>, Error>> {
        let depth = depth.max(1);
        let mut results = Vec::with_capacity(message_ids.len());
        let mut sent = 0;
        while results.len() < message_ids.len() {
            if self.broken {
                let err = Error::Io("connection closed".to_owned());
                results.resize(message_ids.len(), Err(err));
                break;
            }
            let mut batch = Vec::new();
            while sent < message_ids.len() && sent - results.len() < depth {
                batch.extend_from_slice(
                    format!("BODY <{}>\r\n", bare(message_ids[sent].as_ref())).as_bytes(),
                );
                sent += 1;
            }
            if !batch.is_empty()
                && let Err(e) = self.write(&batch).await
            {
                results.resize(message_ids.len(), Err(e));
                break;
            }
            let result = self.read_body().await;
            if let Err(e) = &result
                && !e.connection_usable()
            {
                self.broken = true;
            }
            results.push(result);
        }
        results
    }

    async fn read_body(&mut self) -> Result<Vec<u8>, Error> {
        let (code, message) = self.status().await?;
        match code {
            222 => self.read_block().await,
            _ => Err(self.reply_error(code, message)),
        }
    }

    async fn read_block(&mut self) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        loop {
            self.read_line().await?;
            let line = self.line.as_slice();
            if line == b".\r\n" || line == b".\n" {
                return Ok(out);
            }
            // A line the server dot-stuffed starts with "..": drop the first dot.
            let unstuffed = if line.starts_with(b"..") {
                &line[1..]
            } else {
                line
            };
            out.extend_from_slice(unstuffed);
        }
    }

    /// Sends QUIT and closes the connection.
    pub async fn quit(mut self) {
        if !self.broken && self.write(b"QUIT\r\n").await.is_ok() {
            let _ = self.status().await;
        }
        let _ = self.writer.shutdown().await;
    }

    #[must_use]
    pub fn is_broken(&self) -> bool {
        self.broken
    }

    async fn command(&mut self, command: &str) -> Result<(u16, String), Error> {
        self.write(format!("{command}\r\n").as_bytes()).await?;
        self.status().await
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let result = tokio::time::timeout(self.timeout, async {
            self.writer.write_all(bytes).await?;
            self.writer.flush().await
        })
        .await;
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                self.broken = true;
                Err(io_error(&e))
            }
            Err(_) => {
                self.broken = true;
                Err(Error::Timeout)
            }
        }
    }

    async fn read_line(&mut self) -> Result<(), Error> {
        self.line.clear();
        match tokio::time::timeout(self.timeout, self.reader.read_until(b'\n', &mut self.line))
            .await
        {
            Ok(Ok(0)) => {
                self.broken = true;
                Err(Error::Io("server closed the connection".to_owned()))
            }
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                self.broken = true;
                Err(io_error(&e))
            }
            Err(_) => {
                self.broken = true;
                Err(Error::Timeout)
            }
        }
    }

    async fn status(&mut self) -> Result<(u16, String), Error> {
        self.read_line().await?;
        let text = String::from_utf8_lossy(&self.line);
        let text = text.trim_end();
        let code = text
            .get(..3)
            .and_then(|c| c.parse::<u16>().ok())
            .ok_or_else(|| {
                self.broken = true;
                Error::Io(format!("not an NNTP status line: {text:.80}"))
            })?;
        Ok((code, text.get(4..).unwrap_or_default().to_owned()))
    }

    fn reply_error(&mut self, code: u16, message: String) -> Error {
        match code {
            430 => Error::NoSuchArticle,
            480 => Error::AuthRequired,
            481 | 482 => Error::AuthFailed(message),
            400 | 502 => {
                self.broken = true;
                Error::Unavailable { code, message }
            }
            _ => Error::Protocol { code, message },
        }
    }
}

fn bare(message_id: &str) -> &str {
    message_id
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
}

/// Connections to one server, never more than `max_connections` at a time.
pub struct Pool {
    config: ServerConfig,
    permits: Arc<Semaphore>,
    idle: Arc<Mutex<Vec<Connection>>>,
}

impl Pool {
    #[must_use]
    pub fn new(config: ServerConfig) -> Self {
        let permits = Arc::new(Semaphore::new(config.max_connections.max(1)));
        Self {
            config,
            permits,
            idle: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[must_use]
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Waits for a free slot, then reuses an idle connection or opens a new one.
    ///
    /// # Errors
    /// When a new connection can't be opened.
    ///
    /// # Panics
    /// Only if the idle list's lock was poisoned by a panic elsewhere.
    pub async fn get(&self) -> Result<PooledConnection, Error> {
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|e| Error::Io(e.to_string()))?;
        let reused = self.idle.lock().expect("idle list lock").pop();
        let conn = match reused {
            Some(conn) => conn,
            None => Connection::connect(&self.config).await?,
        };
        Ok(PooledConnection {
            conn: Some(conn),
            idle: Arc::clone(&self.idle),
            _permit: permit,
        })
    }
}

/// A connection borrowed from a [`Pool`]; it goes back when dropped unless it broke.
pub struct PooledConnection {
    conn: Option<Connection>,
    idle: Arc<Mutex<Vec<Connection>>>,
    _permit: OwnedSemaphorePermit,
}

impl std::ops::Deref for PooledConnection {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("connection present until drop")
    }
}

impl std::ops::DerefMut for PooledConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        self.conn.as_mut().expect("connection present until drop")
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take()
            && !conn.broken
            && let Ok(mut idle) = self.idle.lock()
        {
            idle.push(conn);
        }
    }
}
