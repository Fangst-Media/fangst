//! Two engines on 127.0.0.1, one seeding and one downloading, finding each other through a local
//! tracker. Nothing here touches the internet: DHT, local discovery and `UPnP` are off.
//!
//! Assertions use the hash check and the files on disk, never librqbit's transfer counters, which
//! depend on timing (a block can be requested twice, and the live stats vanish once a torrent is
//! no longer live).

use std::path::{Path, PathBuf};
use std::time::Duration;

use fangst_bench::tracker::Tracker;
use fangst_torrent::{Config, Download, Engine, Error, SeedLimits, SeedStop, Seeded};

const TIMEOUT: Duration = Duration::from_mins(1);
const PIECE: u32 = 64 * 1024;
const A_LEN: usize = 1_500_000;
const B_LEN: usize = 700_001;

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

/// Which pieces the seeder holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Seeder {
    Everything,
    /// Every piece except those entirely inside a.bin, so a downloader can only finish if it
    /// keeps the a.bin pieces it already has.
    WithoutA,
}

struct Swarm {
    tracker: Tracker,
    _seeder: Engine,
    seed: Download,
    torrent: Vec<u8>,
    /// See [`a_pieces`].
    a: std::ops::Range<usize>,
    source: PathBuf,
    _dir: tempfile::TempDir,
}

async fn swarm(seeder: Seeder) -> Swarm {
    let dir = tempfile::tempdir().unwrap();
    let tracker = Tracker::start().await.unwrap();
    // Two files whose sizes don't line up with pieces; small pieces so a.bin holds whole ones.
    let source = dir.path().join("source").join("Some.Release");
    write_release(&source);
    let torrent =
        fangst_torrent::create_torrent(&source, vec![tracker.announce_url()], Some(PIECE))
            .await
            .unwrap();
    let a = a_pieces(&torrent);
    let seed_folder = dir.path().join("seed");
    write_release(&seed_folder);
    if seeder == Seeder::WithoutA {
        let mut bytes = std::fs::read(seed_folder.join("a.bin")).unwrap();
        bytes[a.clone()].fill(0);
        std::fs::write(seed_folder.join("a.bin"), bytes).unwrap();
    }
    let engine = Engine::start(config()).await.unwrap();
    let seed = engine.start_torrent(&torrent, &seed_folder).await.unwrap();
    let expected = match seeder {
        Seeder::Everything => seed.total_bytes(),
        Seeder::WithoutA => seed.total_bytes() - a.len() as u64,
    };
    assert_eq!(seed.kept_bytes(), expected);
    tokio::time::timeout(TIMEOUT, async {
        while tracker.peer_count() == 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("seeder announced to the tracker");
    Swarm {
        tracker,
        _seeder: engine,
        seed,
        torrent,
        a,
        source,
        _dir: dir,
    }
}

fn assert_same_files(a: &Path, b: &Path) {
    for name in ["a.bin", "b.bin"] {
        assert!(
            std::fs::read(a.join(name)).unwrap() == std::fs::read(b.join(name)).unwrap(),
            "{name} differs"
        );
    }
}

async fn finish(download: Download) {
    let watch = download.watch();
    tokio::time::timeout(TIMEOUT, download.wait())
        .await
        .expect("finished in time")
        .unwrap();
    assert_eq!(download.have_bytes(), download.total_bytes());
    assert_eq!(watch.progress().have_bytes, download.total_bytes());
    download.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn torrent_file_downloads_from_a_local_seeder() {
    let swarm = swarm(Seeder::Everything).await;
    let out = tempfile::tempdir().unwrap();
    let engine = Engine::start(config()).await.unwrap();

    let download = engine
        .start_torrent(&swarm.torrent, out.path())
        .await
        .unwrap();
    assert_eq!(download.kept_bytes(), 0);
    finish(download).await;

    assert_same_files(&swarm.source, out.path());
    assert!(
        swarm.tracker.peer_count() >= 2,
        "the downloader announced too"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn magnet_resolves_through_the_tracker_then_downloads() {
    let swarm = swarm(Seeder::Everything).await;
    let expected = fangst_torrent::inspect(&swarm.torrent).unwrap();
    let link = format!(
        "magnet:?xt=urn:btih:{}&dn=Some.Release&tr={}",
        expected.info_hash,
        swarm.tracker.announce_url()
    );
    let out = tempfile::tempdir().unwrap();
    let engine = Engine::start(config()).await.unwrap();

    let metadata = tokio::time::timeout(TIMEOUT, engine.resolve(&link))
        .await
        .expect("metadata in time")
        .unwrap();
    assert_eq!(metadata.info_hash, expected.info_hash);
    assert_eq!(metadata.total_bytes, expected.total_bytes);
    assert_eq!(metadata.files, expected.files);

    let download = engine
        .start_torrent(&metadata.torrent, out.path())
        .await
        .unwrap();
    finish(download).await;
    assert_same_files(&swarm.source, out.path());
}

#[tokio::test(flavor = "multi_thread")]
async fn pieces_already_on_disk_are_kept() {
    // The seeder can't supply a.bin's pieces, so finishing proves they came from disk.
    let swarm = swarm(Seeder::WithoutA).await;
    let out = tempfile::tempdir().unwrap();
    // A previous run finished a.bin and stopped before b.bin.
    std::fs::copy(swarm.source.join("a.bin"), out.path().join("a.bin")).unwrap();
    let engine = Engine::start(config()).await.unwrap();

    let download = engine
        .start_torrent(&swarm.torrent, out.path())
        .await
        .unwrap();
    assert_eq!(download.kept_bytes(), swarm.a.len() as u64);
    finish(download).await;
    assert_same_files(&swarm.source, out.path());
}

#[tokio::test(flavor = "multi_thread")]
async fn damaged_pieces_on_disk_are_downloaded_again() {
    let swarm = swarm(Seeder::Everything).await;
    let out = tempfile::tempdir().unwrap();
    let mut a = std::fs::read(swarm.source.join("a.bin")).unwrap();
    // One flipped byte in a.bin's third whole piece fails that piece's check and nothing else.
    a[swarm.a.start + 2 * PIECE as usize + 10] ^= 0xff;
    std::fs::write(out.path().join("a.bin"), a).unwrap();
    let engine = Engine::start(config()).await.unwrap();

    let download = engine
        .start_torrent(&swarm.torrent, out.path())
        .await
        .unwrap();
    assert_eq!(
        download.kept_bytes(),
        swarm.a.len() as u64 - u64::from(PIECE)
    );
    finish(download).await;
    assert_same_files(&swarm.source, out.path());
}

#[tokio::test(flavor = "multi_thread")]
async fn seeding_stops_at_the_ratio_limit() {
    let swarm = swarm(Seeder::Everything).await;
    let total = swarm.seed.total_bytes();
    let clock = swarm.seed.seed_clock(Seeded::default());
    let limits = SeedLimits {
        ratio: Some(1.0),
        time: None,
    };
    // The seeder is the only one holding the pieces, so a full download uploads its whole size.
    let out = tempfile::tempdir().unwrap();
    let leecher = Engine::start(config()).await.unwrap();
    let _download = leecher
        .start_torrent(&swarm.torrent, out.path())
        .await
        .unwrap();

    let stop = tokio::time::timeout(TIMEOUT, swarm.seed.seed(&clock, &limits))
        .await
        .expect("the ratio was reached in time");
    let seeded = clock.seeded();
    assert!(
        matches!(stop, SeedStop::Ratio { limit, ratio } if (limit - 1.0).abs() < f64::EPSILON && ratio >= 1.0),
        "{stop:?}"
    );
    assert!(seeded.uploaded_bytes >= total, "{seeded:?}");
    assert_eq!(stop.to_string().split(' ').next(), Some("ratio"));
}

#[tokio::test(flavor = "multi_thread")]
async fn seeding_stops_at_the_time_limit_when_the_ratio_is_not_reached() {
    let swarm = swarm(Seeder::Everything).await;
    let limit = Duration::from_secs(1);
    let limits = SeedLimits {
        ratio: Some(1.0),
        time: Some(limit),
    };
    let started = std::time::Instant::now();
    let clock = swarm.seed.seed_clock(Seeded::default());
    // Nobody leeches, so nothing is uploaded and only the clock can end seeding.
    let stop = tokio::time::timeout(TIMEOUT, swarm.seed.seed(&clock, &limits))
        .await
        .expect("the time limit ended seeding");
    assert_eq!(stop, SeedStop::Time { limit });
    assert!(started.elapsed() >= limit);
    assert_eq!(clock.seeded().uploaded_bytes, 0);

    // Time counted in an earlier run counts towards the limit straight away.
    let resumed = swarm.seed.seed_clock(Seeded {
        uploaded_bytes: 0,
        time: Duration::from_hours(1),
    });
    let long = SeedLimits {
        ratio: None,
        time: Some(Duration::from_hours(1)),
    };
    let stop = tokio::time::timeout(TIMEOUT, swarm.seed.seed(&resumed, &long))
        .await
        .expect("an earlier run already seeded long enough");
    assert!(matches!(stop, SeedStop::Time { .. }));
}

#[tokio::test]
async fn unsafe_and_broken_input_is_refused() {
    let engine = Engine::start(config()).await.unwrap();
    let out = tempfile::tempdir().unwrap();
    assert!(matches!(
        engine.start_torrent(b"d4:infoe", out.path()).await,
        Err(Error::InvalidTorrent(_))
    ));
    assert!(matches!(
        engine.resolve("magnet:?dn=no-hash").await,
        Err(Error::InvalidMagnet(_))
    ));
}
