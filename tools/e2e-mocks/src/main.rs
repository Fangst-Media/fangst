//! The download side of the adapter end-to-end tests (`tests/adapters`): a usenet server holding
//! every usenet release, a tracker, and a seeder for every torrent release. Each release is one
//! video file named after the release. The NZB and `.torrent` files go to `--out`, where the
//! indexer stub serves them, and `--out/ready` appears once everything is up.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use fangst_bench::tracker::Tracker;
use fangst_bench::{Config, Library, MockServer};

/// yEnc parts this small give a release of a few dozen kilobytes several segments, so the
/// download still exercises joining parts.
const SEGMENT_BYTES: usize = 16 * 1024;
const PIECE_BYTES: u32 = 16 * 1024;
const GROUP: &str = "alt.binaries.fangst";

#[derive(Parser)]
struct Args {
    /// The release list shared with the indexer stub.
    #[arg(long)]
    releases: PathBuf,
    /// The video every release carries.
    #[arg(long)]
    video: PathBuf,
    /// Folder for NZB and torrent files, and for the seeder's copies of the releases.
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value = "0.0.0.0:1119")]
    nntp_listen: SocketAddr,
    #[arg(long, default_value = "0.0.0.0:6969")]
    tracker_listen: SocketAddr,
    /// The announce URL written into every torrent, as the downloader reaches the tracker.
    #[arg(long)]
    announce_url: String,
    #[arg(long, default_value_t = 6881)]
    peer_port: u16,
}

#[derive(serde::Deserialize)]
struct Release {
    title: String,
    protocol: Protocol,
}

#[derive(serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Protocol {
    Usenet,
    Torrent,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let releases: Vec<Release> = serde_json::from_slice(
        &std::fs::read(&args.releases)
            .with_context(|| format!("reading {}", args.releases.display()))?,
    )
    .context("parsing the release list")?;
    let video =
        std::fs::read(&args.video).with_context(|| format!("reading {}", args.video.display()))?;
    std::fs::create_dir_all(&args.out)?;
    let _ = std::fs::remove_file(args.out.join("ready"));

    let mut library = Library::new();
    for release in releases.iter().filter(|r| r.protocol == Protocol::Usenet) {
        let file = format!("{}.mkv", release.title);
        library.add_file(&file, &video, SEGMENT_BYTES, GROUP);
        std::fs::write(
            args.out.join(format!("{}.nzb", release.title)),
            library.nzb_of(&[&file]),
        )?;
    }
    let nntp = MockServer::start_at(args.nntp_listen, library, Config::default()).await?;
    println!("usenet: listening on {}", nntp.addr);

    let tracker = Tracker::start_at(args.tracker_listen).await?;
    println!("tracker: listening on {}", tracker.addr);

    let engine = fangst_torrent::Engine::start(fangst_torrent::Config {
        listen: ([0, 0, 0, 0], args.peer_port).into(),
        announce_port: Some(args.peer_port),
        dht: false,
        local_discovery: false,
        upnp: false,
        state_dir: None,
        download_limit: None,
        upload_limit: None,
    })
    .await?;
    let mut seeds = Vec::new();
    for release in releases.iter().filter(|r| r.protocol == Protocol::Torrent) {
        seeds.push(seed(&engine, &args, &video, &release.title).await?);
    }
    println!(
        "seeder: {} torrents on port {}",
        seeds.len(),
        args.peer_port
    );

    std::fs::write(args.out.join("ready"), b"")?;
    tokio::signal::ctrl_c().await?;
    engine.shutdown().await;
    Ok(())
}

async fn seed(
    engine: &fangst_torrent::Engine,
    args: &Args,
    video: &[u8],
    title: &str,
) -> anyhow::Result<fangst_torrent::Download> {
    let folder = args.out.join("seed").join(title);
    std::fs::create_dir_all(&folder)?;
    std::fs::write(folder.join(format!("{title}.mkv")), video)?;
    let torrent =
        fangst_torrent::create_torrent(&folder, vec![args.announce_url.clone()], Some(PIECE_BYTES))
            .await?;
    std::fs::write(args.out.join(format!("{title}.torrent")), &torrent)?;
    let download = engine.start_torrent(&torrent, &folder).await?;
    anyhow::ensure!(
        download.kept_bytes() == download.total_bytes(),
        "the seeder's copy of {title} failed its hash check"
    );
    Ok(download)
}
