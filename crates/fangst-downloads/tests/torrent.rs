//! Torrent jobs in the shared queue, downloaded from seeders on 127.0.0.1 found through a local
//! tracker. DHT, local discovery and `UPnP` are off, so nothing leaves the machine.
//!
//! Assertions use the hash check, the timeline and the files on disk, never transfer counters or
//! sleeps, so they hold however fast the machine is.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fangst_bench::tracker::Tracker;
use fangst_downloads::{
    CategorySeeding, Error, Event, Kind, Runner, Service, Status, Store, TorrentSource,
};
use fangst_torrent::{Config, Download, Engine};

const TIMEOUT: Duration = Duration::from_mins(1);
const PIECE: u32 = 64 * 1024;
const A_LEN: usize = 1_500_000;
const B_LEN: usize = 700_001;
const TOTAL: u64 = (A_LEN + B_LEN) as u64;

/// Loopback listeners announce no port unless told one, and the tracker needs it to pair peers.
fn config() -> Config {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    Config {
        listen: ([127, 0, 0, 1], port).into(),
        announce_port: Some(port),
        dht: false,
        local_discovery: false,
        upnp: false,
        state_dir: None,
        download_limit: None,
        upload_limit: None,
    }
}

fn data(len: usize, seed: u32) -> Vec<u8> {
    (0..len)
        .map(|i| u32::try_from(i).unwrap().wrapping_mul(seed).to_le_bytes()[2])
        .collect()
}

/// The bytes of a.bin, in a.bin's own offsets, covered by pieces that lie entirely inside it.
/// File order in a generated torrent follows the directory listing, which differs between
/// platforms, so this reads the order from the torrent instead of assuming a.bin comes first.
fn a_pieces(torrent: &[u8]) -> std::ops::Range<usize> {
    let meta = fangst_torrent::inspect(torrent).unwrap();
    let mut offset = 0;
    let mut a_start = None;
    for file in &meta.files {
        if file.path == Path::new("a.bin") {
            a_start = Some(offset);
        }
        offset += usize::try_from(file.len).unwrap();
    }
    let total = offset;
    let a_start = a_start.unwrap();
    let a_end = a_start + A_LEN;
    let piece = PIECE as usize;
    let first = a_start.div_ceil(piece) * piece;
    // The torrent's last piece is short, so it lies inside a.bin whenever a.bin is the last file.
    let last = if a_end == total {
        a_end
    } else {
        a_end / piece * piece
    };
    (first - a_start)..(last - a_start)
}

fn write_release(folder: &Path) {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(folder.join("a.bin"), data(A_LEN, 2_654_435_761)).unwrap();
    std::fs::write(folder.join("b.bin"), data(B_LEN, 40_503)).unwrap();
}

/// Which pieces a seeder holds. Zeroed bytes fail the seeder's own check, so it never offers them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pieces {
    All,
    /// Only the pieces entirely inside a.bin (see [`a_pieces`]).
    OnlyA,
    /// Everything except the pieces entirely inside a.bin.
    WithoutA,
}

struct Swarm {
    tracker: Tracker,
    torrent: Vec<u8>,
    /// See [`a_pieces`].
    a: std::ops::Range<usize>,
    info_hash: String,
    source: PathBuf,
    dir: tempfile::TempDir,
}

impl Swarm {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let tracker = Tracker::start().await.unwrap();
        let source = dir.path().join("source").join("Some.Release");
        write_release(&source);
        // Small pieces so a.bin holds whole ones.
        let torrent =
            fangst_torrent::create_torrent(&source, vec![tracker.announce_url()], Some(PIECE))
                .await
                .unwrap();
        let info_hash = fangst_torrent::inspect(&torrent).unwrap().info_hash;
        let a = a_pieces(&torrent);
        Self {
            tracker,
            torrent,
            a,
            info_hash,
            source,
            dir,
        }
    }

    /// Starts a seeder holding `pieces` and waits until the tracker knows it.
    async fn seeder(&self, pieces: Pieces) -> (Engine, Download) {
        let before = self.tracker.peer_count();
        let folder = self.dir.path().join(format!("seed-{before}"));
        write_release(&folder);
        let mut a = std::fs::read(folder.join("a.bin")).unwrap();
        match pieces {
            Pieces::All => {}
            Pieces::OnlyA => {
                a[..self.a.start].fill(0);
                a[self.a.end..].fill(0);
                std::fs::write(folder.join("b.bin"), vec![0u8; B_LEN]).unwrap();
            }
            Pieces::WithoutA => a[self.a.clone()].fill(0),
        }
        std::fs::write(folder.join("a.bin"), a).unwrap();
        let engine = Engine::start(config()).await.unwrap();
        let seed = engine.start_torrent(&self.torrent, &folder).await.unwrap();
        let expected = match pieces {
            Pieces::All => TOTAL,
            Pieces::OnlyA => self.a.len() as u64,
            Pieces::WithoutA => TOTAL - self.a.len() as u64,
        };
        assert_eq!(seed.kept_bytes(), expected);
        tokio::time::timeout(TIMEOUT, async {
            while self.tracker.peer_count() == before {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("seeder announced to the tracker");
        (engine, seed)
    }
}

/// A service with torrents on and usenet off; the usenet runner never connects anywhere.
fn service(store: Store, root: &Path, engine: Engine) -> Service {
    let runner = Runner {
        store,
        servers: vec![fangst_usenet::Server::primary(Arc::new(
            fangst_nntp::Pool::new(fangst_nntp::ServerConfig {
                host: "127.0.0.1".to_owned(),
                port: 0,
                tls: None,
                credentials: None,
                max_connections: 1,
                connect_timeout: Duration::from_secs(1),
                command_timeout: Duration::from_secs(1),
            }),
        ))],
        root: root.to_owned(),
        options: fangst_usenet::Options {
            connections: 1,
            pipeline_depth: 1,
            retries: 0,
        },
        post_processors: Vec::new(),
    };
    Service::new(runner).with_torrents(engine).without_usenet()
}

async fn wait_for(store: &Store, id: i64, status: Status) -> fangst_downloads::Job {
    tokio::time::timeout(TIMEOUT, async {
        loop {
            let job = store.get(id).await.unwrap();
            if job.status == status {
                return job;
            }
            assert_ne!(job.status, Status::Failed, "{:?}", job.error);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("job {id} never became {}", status.as_str()))
}

fn assert_same_files(a: &Path, b: &Path) {
    for name in ["a.bin", "b.bin"] {
        assert!(
            std::fs::read(a.join(name)).unwrap() == std::fs::read(b.join(name)).unwrap(),
            "{name} differs"
        );
    }
}

fn event<'a>(timeline: &'a [Event], kind: &str) -> Option<&'a Event> {
    timeline.iter().find(|e| e.kind == kind)
}

#[tokio::test]
async fn torrent_jobs_are_checked_and_stored_beside_usenet_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let source = dir.path().join("files");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("x.bin"), data(100_000, 7)).unwrap();
    let torrent = fangst_torrent::create_torrent(&source, Vec::new(), Some(PIECE))
        .await
        .unwrap();

    let file_job = store
        .add_torrent("From file", Some("tv"), 5, TorrentSource::File(&torrent))
        .await
        .unwrap();
    let magnet_job = store
        .add_torrent(
            "From magnet",
            None,
            0,
            TorrentSource::Magnet(
                "magnet:?xt=urn:btih:cab507494d02ebb1178b38f2e9d7be299c86b862&dn=x",
            ),
        )
        .await
        .unwrap();

    let job = store.get(file_job).await.unwrap();
    assert_eq!(job.kind, Kind::Torrent);
    assert_eq!(job.status, Status::Queued);
    assert_eq!(job.total_bytes, 100_000);
    assert_eq!(job.category.as_deref(), Some("tv"));
    assert_eq!(
        store.timeline(file_job).await.unwrap()[0].message,
        "added with 1 files, 100000 bytes"
    );
    assert_eq!(store.get(magnet_job).await.unwrap().total_bytes, 0);
    assert_eq!(
        store
            .next_queued_of(Kind::Torrent)
            .await
            .unwrap()
            .unwrap()
            .id,
        file_job
    );
    assert!(store.next_queued_of(Kind::Usenet).await.unwrap().is_none());

    assert!(matches!(
        store
            .add_torrent("again", None, 0, TorrentSource::File(&torrent))
            .await,
        Err(Error::DuplicateTorrent(id)) if id == file_job
    ));
    assert!(matches!(
        store
            .add_torrent("bad", None, 0, TorrentSource::File(b"not a torrent"))
            .await,
        Err(Error::InvalidTorrent(_))
    ));
    assert!(matches!(
        store
            .add_torrent("bad", None, 0, TorrentSource::Magnet("magnet:?dn=no-hash"))
            .await,
        Err(Error::InvalidTorrent(_))
    ));
    assert_eq!(store.list().await.unwrap().len(), 2);

    // Deleting the job frees the torrent to be added again.
    store.delete(file_job).await.unwrap();
    store
        .add_torrent("again", None, 0, TorrentSource::File(&torrent))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn service_downloads_a_torrent_file_and_a_magnet_link() {
    let swarm = Swarm::new().await;
    let _seeder = swarm.seeder(Pieces::All).await;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let engine = Engine::start(config()).await.unwrap();
    let service = service(store.clone(), &dir.path().join("downloads"), engine);
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });

    let from_file = service
        .add_torrent("From file", None, 0, TorrentSource::File(&swarm.torrent))
        .await
        .unwrap();
    let job = wait_for(&store, from_file, Status::Completed).await;
    assert_same_files(&swarm.source, job.output_path.as_ref().unwrap());
    let status = service.torrent_status(from_file).await.unwrap().unwrap();
    assert_eq!(status.info_hash, swarm.info_hash);
    assert_eq!(
        status.have_bytes, TOTAL,
        "a finished torrent has every byte"
    );
    let mut files: Vec<(String, u64)> = status
        .files
        .iter()
        .map(|f| (f.path.clone(), f.bytes))
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            ("a.bin".to_owned(), A_LEN as u64),
            ("b.bin".to_owned(), B_LEN as u64)
        ]
    );
    let timeline = store.timeline(from_file).await.unwrap();
    assert_eq!(
        event(&timeline, "transfer").unwrap().message,
        format!("downloaded {TOTAL} bytes from peers")
    );
    assert!(event(&timeline, "resumed").is_none());

    // The same torrent can't be two jobs, so the magnet job takes over once the first is gone.
    service.delete(from_file, true).await.unwrap();
    assert!(!job.output_path.unwrap().exists());
    let link = format!(
        "magnet:?xt=urn:btih:{}&dn=Some.Release&tr={}",
        swarm.info_hash,
        swarm.tracker.announce_url()
    );
    let from_magnet = service
        .add_torrent(
            "From magnet",
            Some("movies"),
            0,
            TorrentSource::Magnet(&link),
        )
        .await
        .unwrap();
    let job = wait_for(&store, from_magnet, Status::Completed).await;
    assert_eq!(job.kind, Kind::Torrent);
    assert_eq!(job.total_bytes, 2_200_001);
    assert_same_files(&swarm.source, job.output_path.as_ref().unwrap());
    // Seeding starts on its own after completion; its entries are checked in the seeding tests.
    let kinds: Vec<String> = store
        .timeline(from_magnet)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .filter(|kind| kind != "seeding")
        .collect();
    assert_eq!(
        kinds,
        [
            "added", "status", "metadata", "metadata", "transfer", "status"
        ]
    );
    worker.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_keeps_the_pieces_already_downloaded() {
    let swarm = Swarm::new().await;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("downloads.db");
    let root = dir.path().join("downloads");

    // The first run can only get a.bin's pieces, so it is still downloading when it stops.
    let (first_seeder, _first_seed) = swarm.seeder(Pieces::OnlyA).await;
    let store = Store::open(&db).await.unwrap();
    let first_engine = Engine::start(config()).await.unwrap();
    let first = service(store.clone(), &root, first_engine.clone());
    let id = first
        .add_torrent("Some.Release", None, 0, TorrentSource::File(&swarm.torrent))
        .await
        .unwrap();
    let worker = tokio::spawn({
        let first = first.clone();
        async move { first.run().await }
    });
    let job = wait_for(&store, id, Status::Downloading).await;
    let a_path = job.output_path.unwrap().join("a.bin");
    let want = std::fs::read(swarm.source.join("a.bin")).unwrap();
    tokio::time::timeout(TIMEOUT, async {
        loop {
            if let Ok(have) = std::fs::read(&a_path)
                && have.get(swarm.a.clone()) == Some(&want[swarm.a.clone()])
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the first run wrote a.bin's pieces");
    // The process stops: the queue and the engine go away with the job still downloading.
    worker.abort();
    first_engine.shutdown().await;
    first_seeder.shutdown().await;
    drop(first);
    assert_eq!(store.get(id).await.unwrap().status, Status::Downloading);

    // The only seeder left lacks a.bin's pieces, so finishing proves they were kept from disk.
    let (_second_seeder, _second_seed) = swarm.seeder(Pieces::WithoutA).await;
    let store = Store::open(&db).await.unwrap();
    let second = service(store.clone(), &root, Engine::start(config()).await.unwrap());
    let worker = tokio::spawn({
        let second = second.clone();
        async move { second.run().await }
    });
    let job = wait_for(&store, id, Status::Completed).await;
    worker.abort();

    assert_same_files(&swarm.source, job.output_path.as_ref().unwrap());
    let timeline = store.timeline(id).await.unwrap();
    assert_eq!(
        event(&timeline, "resumed").unwrap().message,
        format!(
            "resuming: {} of {TOTAL} bytes already on disk",
            swarm.a.len()
        )
    );
    let transfers: Vec<&str> = timeline
        .iter()
        .filter(|e| e.kind == "transfer")
        .map(|e| e.message.as_str())
        .collect();
    assert_eq!(
        transfers,
        [format!(
            "downloaded {} bytes from peers",
            TOTAL - swarm.a.len() as u64
        )]
    );
}

/// A magnet link only a local tracker without peers knows never gets its metadata, so its job holds
/// a slot until it is paused.
fn unseeded_magnet(n: u8, tracker: &Tracker) -> String {
    format!(
        "magnet:?xt=urn:btih:{}&dn=Nobody.Seeds.{n}&tr={}",
        format!("{n:02x}").repeat(20),
        tracker.announce_url()
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn no_more_torrents_run_than_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let engine = Engine::start(config()).await.unwrap();
    let tracker = Tracker::start().await.unwrap();
    let service =
        service(store.clone(), &dir.path().join("downloads"), engine).with_torrent_limit(2);
    let mut ids = Vec::new();
    for n in 1..=3 {
        let link = unseeded_magnet(n, &tracker);
        // Falling priority, so the first two are the ones that start.
        let priority = 10 - i64::from(n);
        ids.push(
            service
                .add_torrent("waits", None, priority, TorrentSource::Magnet(&link))
                .await
                .unwrap(),
        );
    }
    let mut changes = service.subscribe();
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });
    wait_for(&store, ids[0], Status::Downloading).await;
    wait_for(&store, ids[1], Status::Downloading).await;
    // The lane would start the third right after the second; give it every chance to.
    let quiet = tokio::time::timeout(Duration::from_millis(500), async {
        while let Ok(change) = changes.recv().await {
            if change == fangst_downloads::Change::Updated(ids[2]) {
                return;
            }
        }
    })
    .await;
    assert!(
        quiet.is_err(),
        "the third torrent changed while two were running"
    );
    assert_eq!(store.get(ids[2]).await.unwrap().status, Status::Queued);
    let waiting = service.torrent_status(ids[2]).await.unwrap().unwrap();
    assert_eq!(waiting.info_hash, "03".repeat(20));
    assert!(waiting.files.is_empty(), "no metadata yet");
    assert_eq!(waiting.have_bytes, 0);
    assert!(
        service
            .torrent_status(ids[0] + 100)
            .await
            .unwrap()
            .is_none()
    );

    service.pause(ids[0]).await.unwrap();
    wait_for(&store, ids[2], Status::Downloading).await;
    assert_eq!(store.get(ids[1]).await.unwrap().status, Status::Downloading);
    worker.abort();
}

fn seeding(ratio_limit: f64, time_limit_secs: i64) -> fangst_core::defaults::Seeding {
    fangst_core::defaults::Seeding {
        ratio_limit,
        time_limit_secs,
    }
}

/// Seeding counts reach the database every second instead of every ten.
fn quick_progress() -> fangst_core::defaults::Progress {
    fangst_core::defaults::Progress {
        publish_interval_ms: 100,
        rate_window_secs: 1,
        persist_interval_secs: 1,
    }
}

/// Waits for a timeline entry of kind `seeding` whose message starts with `prefix`.
async fn seeding_event(store: &Store, id: i64, prefix: &str) -> Event {
    tokio::time::timeout(TIMEOUT, async {
        loop {
            let timeline = store.timeline(id).await.unwrap();
            if let Some(event) = timeline
                .into_iter()
                .find(|e| e.kind == "seeding" && e.message.starts_with(prefix))
            {
                return event;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("job {id} has no seeding entry starting with {prefix:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn seeding_stops_at_the_ratio_limit_in_a_test_swarm() {
    let swarm = Swarm::new().await;
    let (seeder, seed) = swarm.seeder(Pieces::All).await;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let engine = Engine::start(config()).await.unwrap();
    let service = service(store.clone(), &dir.path().join("downloads"), engine)
        .with_progress(quick_progress())
        .with_seeding(seeding(1.0, -1));
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });
    let id = service
        .add_torrent("Some.Release", None, 0, TorrentSource::File(&swarm.torrent))
        .await
        .unwrap();
    let started = seeding_event(&store, id, "seeding").await;
    assert_eq!(started.message, "seeding until ratio 1.00");
    let job = store.get(id).await.unwrap();
    assert_eq!(job.status, Status::Completed);
    assert!(job.seeding);

    // The first seeder leaves, so a new peer can only get the files from the job.
    drop(seed);
    seeder.shutdown().await;
    let leecher = Engine::start(config()).await.unwrap();
    let out = dir.path().join("leecher");
    let download = leecher.start_torrent(&swarm.torrent, &out).await.unwrap();

    let stopped = seeding_event(&store, id, "stopped seeding").await;
    assert!(
        stopped.message.starts_with("stopped seeding: ratio 1.0"),
        "{}",
        stopped.message
    );
    let job = store.get(id).await.unwrap();
    assert_eq!(
        job.status,
        Status::Completed,
        "files stay, the job stays done"
    );
    assert!(!job.seeding);
    assert!(job.uploaded_bytes >= i64::try_from(TOTAL).unwrap());
    let status = service.torrent_status(id).await.unwrap().unwrap();
    assert!(!status.seeding);
    assert!(status.uploaded_bytes >= TOTAL);
    assert_eq!(status.ratio_limit, Some(1.0));
    assert_eq!(status.seeding_time_limit_secs, None);
    tokio::time::timeout(TIMEOUT, download.wait())
        .await
        .expect("the leecher got everything the job sent")
        .unwrap();
    assert_same_files(&swarm.source, &out);
    worker.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn seeding_a_category_override_beats_the_default() {
    let swarm = Swarm::new().await;
    let _seeder = swarm.seeder(Pieces::All).await;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let engine = Engine::start(config()).await.unwrap();
    // By default nothing would ever stop seeding.
    let service = service(store.clone(), &dir.path().join("downloads"), engine)
        .with_progress(quick_progress())
        .with_seeding(seeding(-1.0, -1));
    let short = CategorySeeding {
        ratio_limit: Some(5.0),
        time_limit_secs: Some(1),
    };
    service.set_category_seeding("short", short).await.unwrap();
    assert_eq!(service.category_seeding("short").await.unwrap(), short);
    assert_eq!(
        service.category_seeding("other").await.unwrap(),
        CategorySeeding::default()
    );
    assert!(
        service
            .categories()
            .await
            .unwrap()
            .contains(&"short".to_owned())
    );
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });
    let id = service
        .add_torrent(
            "Some.Release",
            Some("short"),
            0,
            TorrentSource::File(&swarm.torrent),
        )
        .await
        .unwrap();

    let started = seeding_event(&store, id, "seeding").await;
    assert_eq!(
        started.message,
        "seeding until ratio 5.00 or for 1 seconds, whichever comes first"
    );
    // Nobody leeches, so only the category's time limit can end it.
    let stopped = seeding_event(&store, id, "stopped seeding").await;
    assert!(
        stopped
            .message
            .starts_with("stopped seeding: seeded for the limit of 1 seconds"),
        "{}",
        stopped.message
    );
    let job = store.get(id).await.unwrap();
    assert!(!job.seeding);
    assert!(job.seeding_secs >= 1);
    let status = service.torrent_status(id).await.unwrap().unwrap();
    assert_eq!(status.ratio_limit, Some(5.0));
    assert_eq!(status.seeding_time_limit_secs, Some(1));
    worker.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn seeding_restart_keeps_the_seed_time_counted() {
    let swarm = Swarm::new().await;
    let _seeder = swarm.seeder(Pieces::All).await;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("downloads.db");
    let root = dir.path().join("downloads");

    // The first daemon runs on a runtime of its own, so dropping it ends every task it started,
    // seeding included, the way a process that stops does.
    let first = std::thread::spawn({
        let torrent = swarm.torrent.clone();
        let db = db.clone();
        let root = root.clone();
        move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let id = runtime.block_on(async move {
                let store = Store::open(&db).await.unwrap();
                let engine = Engine::start(config()).await.unwrap();
                let service = service(store.clone(), &root, engine)
                    .with_progress(quick_progress())
                    .with_seeding(seeding(-1.0, -1));
                let running = service.clone();
                tokio::spawn(async move { running.run().await });
                let id = service
                    .add_torrent("Some.Release", None, 0, TorrentSource::File(&torrent))
                    .await
                    .unwrap();
                seeding_event(&store, id, "seeding").await;
                // A second handle on the file sees only what was written, not the live count.
                let written = Store::open(&db).await.unwrap();
                tokio::time::timeout(TIMEOUT, async {
                    while written.get(id).await.unwrap().seeding_secs < 2 {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                })
                .await
                .expect("seed time was written");
                id
            });
            runtime.shutdown_timeout(Duration::from_secs(10));
            id
        }
    });
    let id = tokio::task::spawn_blocking(move || first.join().unwrap())
        .await
        .unwrap();

    let store = Store::open(&db).await.unwrap();
    let job = store.get(id).await.unwrap();
    assert!(
        job.seeding,
        "the job was still seeding when the daemon stopped"
    );
    let counted = job.seeding_secs;
    assert!(counted >= 2);

    // Two more seconds of seeding reach the limit only if the time already counted counts.
    let limit = counted + 2;
    let second = service(store.clone(), &root, Engine::start(config()).await.unwrap())
        .with_progress(quick_progress())
        .with_seeding(seeding(-1.0, limit));
    let worker = tokio::spawn({
        let second = second.clone();
        async move { second.run().await }
    });
    let resumed = seeding_event(&store, id, "seeding again").await;
    assert_eq!(
        resumed.message,
        format!("seeding again for {limit} seconds, {counted} seconds already counted")
    );
    let stopped = seeding_event(&store, id, "stopped seeding").await;
    assert!(
        stopped.message.starts_with(&format!(
            "stopped seeding: seeded for the limit of {limit} seconds"
        )),
        "{}",
        stopped.message
    );
    let job = store.get(id).await.unwrap();
    assert!(!job.seeding);
    assert!(job.seeding_secs >= limit);
    assert_eq!(job.status, Status::Completed);
    worker.abort();
}
