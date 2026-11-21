//! A minimal HTTP `BitTorrent` tracker for tests: it remembers who announced each info hash and
//! hands every announcer the others, all on 127.0.0.1.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Peers announce often enough in a test that a short interval only matters for slow starters.
const INTERVAL_SECS: u32 = 2;

type Swarms = Arc<Mutex<HashMap<String, Vec<SocketAddr>>>>;

pub struct Tracker {
    pub addr: SocketAddr,
    swarms: Swarms,
    task: JoinHandle<()>,
}

impl Drop for Tracker {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Tracker {
    /// # Errors
    /// When no local port can be bound.
    pub async fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let swarms = Swarms::default();
        let shared = Arc::clone(&swarms);
        let task = tokio::spawn(async move {
            while let Ok((stream, peer)) = listener.accept().await {
                tokio::spawn(serve(stream, peer, Arc::clone(&shared)));
            }
        });
        Ok(Self { addr, swarms, task })
    }

    #[must_use]
    pub fn announce_url(&self) -> String {
        format!("http://{}/announce", self.addr)
    }

    /// How many distinct peers have announced any torrent.
    ///
    /// # Panics
    /// If the swarm lock was poisoned.
    #[must_use]
    pub fn peer_count(&self) -> usize {
        self.swarms
            .lock()
            .expect("swarms")
            .values()
            .map(Vec::len)
            .sum()
    }
}

async fn serve(mut stream: tokio::net::TcpStream, peer: SocketAddr, swarms: Swarms) {
    let mut request = Vec::new();
    let mut buf = [0u8; 4096];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => request.extend_from_slice(&buf[..n]),
        }
    }
    let text = String::from_utf8_lossy(&request);
    let target = text.split_whitespace().nth(1).unwrap_or_default();
    let query = target.split_once('?').map_or("", |(_, q)| q);
    let param = |name: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
            .map(str::to_owned)
    };
    let (Some(info_hash), Some(port)) = (
        param("info_hash"),
        param("port").and_then(|p| p.parse::<u16>().ok()),
    ) else {
        let _ = stream
            .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
            .await;
        return;
    };
    let announcer = SocketAddr::new(peer.ip(), port);
    let others: Vec<SocketAddr> = {
        let mut swarms = swarms.lock().expect("swarms");
        let swarm = swarms.entry(info_hash).or_default();
        if !swarm.contains(&announcer) {
            swarm.push(announcer);
        }
        swarm.iter().filter(|a| **a != announcer).copied().collect()
    };

    let mut compact = Vec::new();
    for other in others {
        if let std::net::IpAddr::V4(ip) = other.ip() {
            compact.extend_from_slice(&ip.octets());
            compact.extend_from_slice(&other.port().to_be_bytes());
        }
    }
    let mut body = format!("d8:intervali{INTERVAL_SECS}e5:peers{}:", compact.len()).into_bytes();
    body.extend_from_slice(&compact);
    body.push(b'e');
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    let _ = stream.write_all(&response).await;
}
