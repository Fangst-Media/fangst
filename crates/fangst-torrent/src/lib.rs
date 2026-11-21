//! Downloads magnet links and `.torrent` files.
//!
//! A thin layer over librqbit. The queue in `fangst-downloads` owns what is downloaded and where;
//! this crate only checks torrent metadata at the edge, resolves magnet links to metadata, and runs
//! one torrent into one folder. Nothing is persisted here: a restart adds the torrent again and
//! librqbit hashes the files already on disk, so only pieces that fail the check are fetched again.

use std::fmt;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use librqbit::dht::DhtPersistenceConfig;
use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, DhtSessionConfig, ListenerMode,
    ListenerOptions, ManagedTorrent, Session, SessionOptions, TorrentStatsState,
};

/// librqbit reports completion by state changes it doesn't expose as an event we can await
/// together with errors, so the download is polled. A quarter second keeps the delay after the
/// last piece invisible next to any real download.
const POLL: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub enum Error {
    InvalidMagnet(String),
    InvalidTorrent(String),
    /// A file name in the metadata would land outside the job folder.
    UnsafePath(String),
    /// The engine refused or lost the torrent.
    Engine(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagnet(e) => write!(f, "invalid magnet link: {e}"),
            Self::InvalidTorrent(e) => write!(f, "invalid torrent: {e}"),
            Self::UnsafePath(p) => write!(f, "torrent names a file outside its folder: {p}"),
            Self::Engine(e) => write!(f, "torrent engine: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// librqbit reports errors as `anyhow::Error`; the alternate form keeps the whole cause chain.
fn engine(e: &impl fmt::Display) -> Error {
    Error::Engine(format!("{e:#}"))
}

/// What a magnet link says before any peer has been asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagnetLink {
    /// Lowercase hex of the v1 info hash.
    pub info_hash: String,
    /// The `dn` display name, if the link carries one.
    pub name: Option<String>,
}

/// Checks a magnet link. Only v1 (`btih`) links are accepted, since jobs are keyed by that hash.
///
/// # Errors
/// [`Error::InvalidMagnet`] when the text is not a magnet link with a v1 info hash.
pub fn parse_magnet(link: &str) -> Result<MagnetLink, Error> {
    if !link.starts_with("magnet:") {
        return Err(Error::InvalidMagnet("must start with magnet:".to_owned()));
    }
    let magnet = librqbit::Magnet::parse(link).map_err(|e| Error::InvalidMagnet(e.to_string()))?;
    let hash = magnet
        .as_id20()
        .ok_or_else(|| Error::InvalidMagnet("no v1 (btih) info hash".to_owned()))?;
    Ok(MagnetLink {
        info_hash: hash.as_string(),
        name: magnet.name,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Relative to the job folder.
    pub path: PathBuf,
    pub len: u64,
}

/// The checked contents of a `.torrent` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// Lowercase hex of the v1 info hash.
    pub info_hash: String,
    pub name: Option<String>,
    pub total_bytes: u64,
    pub files: Vec<FileEntry>,
    /// The `.torrent` bytes, kept so a restart never needs peers for the metadata again.
    pub torrent: Vec<u8>,
}

/// Parses and checks a `.torrent` file: valid bencode, consistent lengths, and file names that stay
/// inside the job folder on every platform.
///
/// # Errors
/// [`Error::InvalidTorrent`] or [`Error::UnsafePath`].
pub fn inspect(torrent: &[u8]) -> Result<Metadata, Error> {
    let parsed =
        librqbit::torrent_from_bytes(torrent).map_err(|e| Error::InvalidTorrent(e.to_string()))?;
    let info_hash = parsed.info_hash.as_string();
    let info = parsed
        .info
        .data
        .validate()
        .map_err(|e| Error::InvalidTorrent(e.to_string()))?;
    let files = info
        .iter_file_details()
        .map(|file| {
            let parts = file.filename.to_vec();
            for part in &parts {
                check_component(part)?;
            }
            Ok(FileEntry {
                path: parts.iter().collect(),
                len: file.len,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Metadata {
        info_hash,
        name: info.name().map(std::borrow::Cow::into_owned),
        total_bytes: info.lengths().total_length(),
        files,
        torrent: torrent.to_vec(),
    })
}

/// librqbit refuses `..` and separators, but a component such as `C:` is a drive prefix on Windows
/// and would make the joined path absolute.
fn check_component(part: &str) -> Result<(), Error> {
    let mut components = Path::new(part).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(Error::UnsafePath(part.to_owned())),
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Where peers connect in. Port 0 picks a free port.
    pub listen: SocketAddr,
    /// The port told to trackers and the DHT, when it differs from the listen port (a forwarded
    /// port), or when listening on loopback, where librqbit announces no port by default.
    pub announce_port: Option<u16>,
    /// Find peers through the DHT, the tracker-less peer network.
    pub dht: bool,
    /// Find peers on the local network by multicast.
    pub local_discovery: bool,
    /// Ask the router to forward the listen port.
    pub upnp: bool,
    /// Folder for the engine's own state, such as known DHT nodes. `None` keeps it in memory.
    pub state_dir: Option<PathBuf>,
    /// Bytes per second across all torrents; `None` is unlimited.
    pub download_limit: Option<NonZeroU32>,
    pub upload_limit: Option<NonZeroU32>,
}

/// One torrent session shared by every torrent job.
#[derive(Clone)]
pub struct Engine {
    session: Arc<Session>,
}

impl Engine {
    /// # Errors
    /// [`Error::Engine`] when the listen port can't be bound.
    pub async fn start(config: Config) -> Result<Self, Error> {
        let dht = config.dht.then(|| DhtSessionConfig {
            bootstrap_addrs: None,
            port: None,
            persistence: config.state_dir.as_ref().map(|dir| DhtPersistenceConfig {
                dump_interval: None,
                config_filename: Some(dir.join("dht.json")),
            }),
        });
        let options = SessionOptions {
            dht,
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpOnly,
                listen_addr: config.listen,
                enable_upnp_port_forwarding: config.upnp,
                announce_port: config.announce_port,
                ..ListenerOptions::default()
            }),
            disable_local_service_discovery: !config.local_discovery,
            ratelimits: LimitsConfig {
                upload_bps: config.upload_limit,
                download_bps: config.download_limit,
            },
            ..SessionOptions::default()
        };
        // Every torrent is added with its own output folder, so the session default is never written to.
        let fallback = config.state_dir.unwrap_or_else(std::env::temp_dir);
        let session = Session::new_with_opts(fallback, options)
            .await
            .map_err(|e| engine(&e))?;
        Ok(Self { session })
    }

    /// The address peers connect to.
    #[must_use]
    pub fn listen_addr(&self) -> Option<SocketAddr> {
        self.session.listen_addr()
    }

    /// Asks peers (from the link's trackers and the DHT) for a magnet link's metadata. Waits until
    /// a peer answers; cancel by dropping the future.
    ///
    /// # Errors
    /// When the link is invalid, the engine fails, or the metadata is unsafe.
    pub async fn resolve(&self, link: &str) -> Result<Metadata, Error> {
        parse_magnet(link)?;
        let response = self
            .session
            .add_torrent(
                AddTorrent::from_url(link),
                Some(AddTorrentOptions {
                    list_only: true,
                    ..AddTorrentOptions::default()
                }),
            )
            .await
            .map_err(|e| engine(&e))?;
        match response {
            AddTorrentResponse::ListOnly(listed) => inspect(&listed.torrent_bytes),
            AddTorrentResponse::AlreadyManaged(_, handle)
            | AddTorrentResponse::Added(_, handle) => handle
                .with_metadata(|m| inspect(&m.torrent_bytes))
                .map_err(|e| engine(&e))?,
        }
    }

    /// Starts a torrent into `folder` and returns once the files on disk have been checked.
    /// Pieces already there and intact are kept; a complete folder makes this a seed.
    ///
    /// # Errors
    /// When the metadata is invalid or unsafe, or the engine can't open the files.
    pub async fn start_torrent(&self, torrent: &[u8], folder: &Path) -> Result<Download, Error> {
        let metadata = inspect(torrent)?;
        let add = || {
            self.session.add_torrent(
                AddTorrent::from_bytes(torrent.to_vec()),
                Some(AddTorrentOptions {
                    // Files from an earlier run are what resuming builds on; librqbit checks them.
                    overwrite: true,
                    // Started paused so the check result can be read before peers add to it.
                    paused: true,
                    output_folder: Some(folder.to_string_lossy().into_owned()),
                    ..AddTorrentOptions::default()
                }),
            )
        };
        let handle = match add().await.map_err(|e| engine(&e))? {
            AddTorrentResponse::Added(_, handle) => handle,
            // A paused or deleted job's torrent is removed in the background; finish that first.
            AddTorrentResponse::AlreadyManaged(id, _) => {
                let _ = self.session.delete(id.into(), false).await;
                add()
                    .await
                    .map_err(|e| engine(&e))?
                    .into_handle()
                    .ok_or_else(|| Error::Engine("torrent was not added".to_owned()))?
            }
            AddTorrentResponse::ListOnly(_) => {
                return Err(Error::Engine("torrent was not added".to_owned()));
            }
        };
        let mut download = Download {
            session: Arc::clone(&self.session),
            handle,
            total_bytes: metadata.total_bytes,
            kept_bytes: 0,
            stopped: false,
        };
        download
            .handle
            .wait_until_initialized()
            .await
            .map_err(|e| engine(&e))?;
        download.kept_bytes = download.handle.stats().progress_bytes;
        self.session
            .unpause(&download.handle)
            .await
            .map_err(|e| engine(&e))?;
        Ok(download)
    }

    /// Removes a torrent by info hash and waits until its files are closed, so the caller can
    /// delete them. Does nothing when the engine doesn't hold it.
    pub async fn forget(&self, info_hash: &str) {
        if let Ok(hash) = info_hash.parse::<librqbit::dht::Id20>() {
            let _ = self.session.delete(hash.into(), false).await;
        }
    }

    /// Stops every torrent and closes the listen port.
    pub async fn shutdown(&self) {
        self.session.stop().await;
    }
}

/// One running torrent. Dropping it removes the torrent from the engine and keeps the files.
pub struct Download {
    session: Arc<Session>,
    handle: Arc<ManagedTorrent>,
    total_bytes: u64,
    kept_bytes: u64,
    stopped: bool,
}

impl Download {
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Bytes that were already on disk and passed the hash check when the torrent started; these
    /// pieces are never requested from peers.
    #[must_use]
    pub fn kept_bytes(&self) -> u64 {
        self.kept_bytes
    }

    /// Bytes on disk that have passed the hash check so far.
    #[must_use]
    pub fn have_bytes(&self) -> u64 {
        self.handle.stats().progress_bytes
    }

    /// Reads [`Download::have_bytes`] without holding on to the download, for progress reports
    /// taken while another task waits on it.
    pub fn have_bytes_reader(&self) -> impl Fn() -> u64 + Send + Sync + 'static {
        let handle = Arc::clone(&self.handle);
        move || handle.stats().progress_bytes
    }

    /// A handle that reads this torrent's progress while it runs, for callers that don't own it.
    #[must_use]
    pub fn watch(&self) -> Watch {
        Watch {
            handle: Arc::clone(&self.handle),
        }
    }

    /// Waits until every piece is on disk and checked.
    ///
    /// # Errors
    /// [`Error::Engine`] when the torrent stops with an error, such as a full disk.
    pub async fn wait(&self) -> Result<(), Error> {
        let mut tick = tokio::time::interval(POLL);
        loop {
            tick.tick().await;
            let stats = self.handle.stats();
            if let TorrentStatsState::Error = stats.state {
                return Err(Error::Engine(
                    stats.error.unwrap_or_else(|| "unknown error".to_owned()),
                ));
            }
            if stats.finished {
                return Ok(());
            }
        }
    }

    /// Starts counting seed time from now, on top of what earlier runs counted. Uploads count from
    /// when the torrent started in this run, downloading included, so `earlier` must hold only what
    /// runs before this one sent.
    #[must_use]
    pub fn seed_clock(&self, earlier: Seeded) -> SeedClock {
        SeedClock {
            handle: Arc::clone(&self.handle),
            earlier,
            started: Instant::now(),
        }
    }

    /// Seeds until `limits` is reached or the engine fails, checking a few times a second. Peers
    /// are served all along; this only watches. Cancel by dropping the future, for instance to
    /// record progress or apply new limits, then call it again with the same clock.
    pub async fn seed(&self, clock: &SeedClock, limits: &SeedLimits) -> SeedStop {
        let mut tick = tokio::time::interval(POLL);
        loop {
            tick.tick().await;
            if let Some(error) = clock.error() {
                return SeedStop::Failed(error);
            }
            if let Some(stop) = limits.reached(clock.seeded(), self.total_bytes) {
                return stop;
            }
        }
    }

    /// Removes the torrent from the engine, closing its files, and keeps what is on disk.
    pub async fn stop(mut self) {
        self.stopped = true;
        let _ = self.session.delete(self.handle.id().into(), false).await;
    }
}

/// Where a running torrent stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    /// Bytes on disk that passed the hash check.
    pub have_bytes: u64,
    /// Bytes sent to peers since the torrent started in this run.
    pub uploaded_bytes: u64,
    /// Bytes per second over the last few seconds; 0 while not exchanging data.
    pub download_bps: u64,
    pub upload_bps: u64,
}

/// Reads a running torrent's [`Progress`]. It holds the engine's handle to the torrent, so drop it
/// when the torrent stops.
#[derive(Clone)]
pub struct Watch {
    handle: Arc<ManagedTorrent>,
}

impl Watch {
    #[must_use]
    pub fn progress(&self) -> Progress {
        let stats = self.handle.stats();
        let (download_bps, upload_bps) = stats.live.as_ref().map_or((0, 0), |live| {
            (live.download_speed.as_bytes(), live.upload_speed.as_bytes())
        });
        Progress {
            have_bytes: stats.progress_bytes,
            uploaded_bytes: stats.uploaded_bytes,
            download_bps,
            upload_bps,
        }
    }
}

/// What seeding has counted for one torrent, across runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Seeded {
    /// Bytes sent to peers, downloading included.
    pub uploaded_bytes: u64,
    /// Time spent seeding after the download finished.
    pub time: Duration,
}

impl Seeded {
    /// Uploaded bytes over the torrent's size; 0 for an empty torrent.
    #[must_use]
    pub fn ratio(&self, total_bytes: u64) -> f64 {
        if total_bytes == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)] // A ratio; 2^52 bytes is far away.
        let ratio = self.uploaded_bytes as f64 / total_bytes as f64;
        ratio
    }
}

/// When a finished torrent stops seeding: at the ratio or after the time, whichever comes first.
/// `None` turns that limit off; with both off a torrent seeds until it is deleted.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SeedLimits {
    /// Uploaded bytes over the torrent's size.
    pub ratio: Option<f64>,
    pub time: Option<Duration>,
}

/// Why seeding ended.
#[derive(Debug, Clone, PartialEq)]
pub enum SeedStop {
    Ratio {
        ratio: f64,
        limit: f64,
    },
    Time {
        limit: Duration,
    },
    /// The engine stopped the torrent, for instance because its files went missing.
    Failed(String),
}

impl fmt::Display for SeedStop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ratio { ratio, limit } => {
                write!(f, "ratio {ratio:.2} reached the limit of {limit:.2}")
            }
            Self::Time { limit } => {
                write!(f, "seeded for the limit of {} seconds", limit.as_secs())
            }
            Self::Failed(e) => write!(f, "torrent engine: {e}"),
        }
    }
}

impl SeedLimits {
    /// Which limit `seeded` has reached for a torrent of `total_bytes`, ratio first.
    #[must_use]
    pub fn reached(&self, seeded: Seeded, total_bytes: u64) -> Option<SeedStop> {
        if let Some(limit) = self.ratio {
            let ratio = seeded.ratio(total_bytes);
            // Compared as bytes so a ratio of exactly 1.0 is reached by sending exactly the size.
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            let needed = (limit.max(0.0) * total_bytes as f64).ceil() as u64;
            if seeded.uploaded_bytes >= needed {
                return Some(SeedStop::Ratio { ratio, limit });
            }
        }
        if let Some(limit) = self.time
            && seeded.time >= limit
        {
            return Some(SeedStop::Time { limit });
        }
        None
    }
}

/// Counts a seeding torrent's uploads and seed time; cheap to clone, for readers elsewhere.
#[derive(Clone)]
pub struct SeedClock {
    handle: Arc<ManagedTorrent>,
    earlier: Seeded,
    started: Instant,
}

impl SeedClock {
    /// Everything counted so far, earlier runs included.
    #[must_use]
    pub fn seeded(&self) -> Seeded {
        Seeded {
            uploaded_bytes: self.earlier.uploaded_bytes + self.handle.stats().uploaded_bytes,
            time: self.earlier.time + self.started.elapsed(),
        }
    }

    /// Bytes per second sent to peers right now.
    #[must_use]
    pub fn upload_bps(&self) -> u64 {
        self.handle
            .stats()
            .live
            .as_ref()
            .map_or(0, |live| live.upload_speed.as_bytes())
    }

    fn error(&self) -> Option<String> {
        let stats = self.handle.stats();
        match stats.state {
            TorrentStatsState::Error => {
                Some(stats.error.unwrap_or_else(|| "unknown error".to_owned()))
            }
            _ => None,
        }
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        if self.stopped {
            return;
        }
        // Dropped mid-download (a job paused or deleted): the files must be released all the same.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let session = Arc::clone(&self.session);
            let id = self.handle.id();
            runtime.spawn(async move {
                let _ = session.delete(id.into(), false).await;
            });
        }
    }
}

/// Builds a `.torrent` for a file or folder. Used to create test fixtures; a user-facing
/// "create torrent" feature would start here. `piece_length` defaults to librqbit's choice (2 MiB).
///
/// # Errors
/// When the path can't be read.
pub async fn create_torrent(
    path: &Path,
    trackers: Vec<String>,
    piece_length: Option<u32>,
) -> Result<Vec<u8>, Error> {
    let result = librqbit::create_torrent(
        path,
        librqbit::CreateTorrentOptions {
            trackers,
            piece_length,
            ..librqbit::CreateTorrentOptions::default()
        },
        &librqbit::spawn_utils::BlockingSpawner::new(1),
    )
    .await
    .map_err(|e| engine(&e))?;
    let bytes = result.as_bytes().map_err(|e| engine(&e))?;
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_prefixes_and_parents_are_unsafe() {
        assert!(check_component("movie.mkv").is_ok());
        assert!(check_component("..").is_err());
        assert!(check_component("").is_err());
        #[cfg(windows)]
        assert!(check_component("C:").is_err());
        #[cfg(unix)]
        assert!(check_component("/").is_err());
    }

    #[test]
    fn magnet_needs_a_v1_hash() {
        let link = parse_magnet(
            "magnet:?xt=urn:btih:cab507494d02ebb1178b38f2e9d7be299c86b862&dn=Some%20Show",
        )
        .unwrap();
        assert_eq!(link.info_hash, "cab507494d02ebb1178b38f2e9d7be299c86b862");
        assert_eq!(link.name.as_deref(), Some("Some Show"));
        assert!(matches!(
            parse_magnet("https://example.com/a.torrent"),
            Err(Error::InvalidMagnet(_))
        ));
        assert!(matches!(
            parse_magnet("magnet:?dn=nothing"),
            Err(Error::InvalidMagnet(_))
        ));
    }

    #[test]
    fn seeding_limits_whichever_comes_first() {
        let size = 1_000;
        let seeded = |uploaded_bytes, secs| Seeded {
            uploaded_bytes,
            time: Duration::from_secs(secs),
        };
        let both = SeedLimits {
            ratio: Some(1.0),
            time: Some(Duration::from_mins(1)),
        };
        assert_eq!(both.reached(seeded(999, 59), size), None);
        assert!(matches!(
            both.reached(seeded(1_000, 0), size),
            Some(SeedStop::Ratio { .. })
        ));
        assert_eq!(
            both.reached(seeded(10, 60), size),
            Some(SeedStop::Time {
                limit: Duration::from_mins(1)
            })
        );
        let none = SeedLimits::default();
        assert_eq!(none.reached(seeded(u64::MAX / 2, 1 << 40), size), None);
        let zero = SeedLimits {
            ratio: Some(0.0),
            time: None,
        };
        assert!(zero.reached(seeded(0, 0), size).is_some());
        assert!((seeded(1_500, 0).ratio(size) - 1.5).abs() < 1e-9);
        assert!(seeded(1_500, 0).ratio(0).abs() < f64::EPSILON);
    }

    #[test]
    fn garbage_is_not_a_torrent() {
        assert!(matches!(
            inspect(b"not bencode"),
            Err(Error::InvalidTorrent(_))
        ));
    }
}
