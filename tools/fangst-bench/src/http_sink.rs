//! A local HTTP server that records every request and answers each with one fixed status, for
//! testing anything that sends webhooks.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    pub method: String,
    /// Path and query, as sent.
    pub path: String,
    /// Names in lowercase.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RecordedRequest {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

pub struct HttpSink {
    pub addr: SocketAddr,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    task: JoinHandle<()>,
}

impl HttpSink {
    /// Starts on a free port; every request gets `status` and an empty body.
    ///
    /// # Errors
    /// When no local port can be bound.
    pub async fn start(status: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let recorded = Arc::clone(&recorded);
                tokio::spawn(async move {
                    if let (Ok(request), Ok(mut recorded)) =
                        (serve(stream, status).await, recorded.lock())
                    {
                        recorded.push(request);
                    }
                });
            }
        });
        Ok(Self {
            addr,
            requests,
            task,
        })
    }

    #[must_use]
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// # Panics
    /// If a connection task panicked while holding the lock.
    #[must_use]
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("sink lock").clone()
    }

    /// Waits until at least `count` requests arrived or `limit` passed, then returns them all.
    ///
    /// # Panics
    /// See [`HttpSink::requests`].
    pub async fn wait_for(&self, count: usize, limit: Duration) -> Vec<RecordedRequest> {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let requests = self.requests();
            if requests.len() >= count || tokio::time::Instant::now() >= deadline {
                return requests;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

impl Drop for HttpSink {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(stream: TcpStream, status: u16) -> std::io::Result<RecordedRequest> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    let mut headers = Vec::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
    }
    let length = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    let response =
        format!("HTTP/1.1 {status} Sink\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let mut stream = reader.into_inner();
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(RecordedRequest {
        method,
        path,
        headers,
        body,
    })
}
