//! Torrent jobs: what the store keeps about them and how one runs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fangst_torrent::{Engine, Metadata, Watch};

use crate::progress::Meter;
use crate::{Error, Job, Kind, Status, Store, now};

/// What a user hands in to start a torrent job.
#[derive(Debug, Clone, Copy)]
pub enum TorrentSource<'a> {
    Magnet(&'a str),
    /// The bytes of a `.torrent` file.
    File(&'a [u8]),
}

/// Where a torrent job stands, for callers that show torrents the way torrent clients do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentStatus {
    /// Lowercase hex of the v1 info hash.
    pub info_hash: String,
    /// Empty until the metadata is known, which for a magnet link means until a peer sends it.
    pub files: Vec<TorrentFile>,
    /// Bytes on disk that passed the hash check: live while running, all of them once completed.
    pub have_bytes: u64,
    /// Bytes sent to peers in the current run; 0 when not running.
    pub uploaded_bytes: u64,
    pub download_bps: u64,
    pub upload_bps: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentFile {
    /// Relative to the job folder, with `/` between folders on every platform.
    pub path: String,
    pub bytes: u64,
}

/// Running torrents by job id, so their progress can be read while they download.
#[derive(Clone, Default)]
pub(crate) struct Live(Arc<Mutex<HashMap<i64, Watch>>>);

/// Removes its job from [`Live`] when dropped, which also covers a job task that is aborted.
struct Watching {
    live: Live,
    id: i64,
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.live.0.lock().expect("live lock").remove(&self.id);
    }
}

impl Live {
    fn watch(&self, id: i64, watch: Watch) -> Watching {
        self.0.lock().expect("live lock").insert(id, watch);
        Watching {
            live: self.clone(),
            id,
        }
    }

    pub(crate) fn progress(&self, id: i64) -> Option<fangst_torrent::Progress> {
        self.0
            .lock()
            .expect("live lock")
            .get(&id)
            .map(Watch::progress)
    }
}

/// The status of a torrent job, or `None` for an unknown id or a usenet job.
pub(crate) async fn status(
    store: &Store,
    live: Option<&Live>,
    id: i64,
) -> Result<Option<TorrentStatus>, Error> {
    let job = match store.get(id).await {
        Ok(job) => job,
        Err(Error::NotFound(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    if job.kind != Kind::Torrent {
        return Ok(None);
    }
    let info_hash = store.torrent_info_hash(id).await?;
    let row = store.torrent(id).await?;
    // The metadata was checked when it was stored; a row that no longer parses lists no files.
    let files = row
        .metainfo
        .and_then(|bytes| fangst_torrent::inspect(&bytes).ok())
        .map(|meta| {
            meta.files
                .into_iter()
                .map(|file| TorrentFile {
                    path: file
                        .path
                        .iter()
                        .map(|part| part.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                    bytes: file.len,
                })
                .collect()
        })
        .unwrap_or_default();
    let total = u64::try_from(job.total_bytes).unwrap_or(0);
    let progress = live.and_then(|live| live.progress(id));
    Ok(Some(TorrentStatus {
        info_hash,
        files,
        have_bytes: progress.map_or_else(
            || {
                if job.status == Status::Completed {
                    total
                } else {
                    0
                }
            },
            |p| p.have_bytes,
        ),
        uploaded_bytes: progress.map_or(0, |p| p.uploaded_bytes),
        download_bps: progress.map_or(0, |p| p.download_bps),
        upload_bps: progress.map_or(0, |p| p.upload_bps),
    }))
}

struct TorrentRow {
    magnet: Option<String>,
    metainfo: Option<Vec<u8>>,
}

impl Store {
    /// Queues a magnet link or `.torrent` file after checking it. A `.torrent` is checked fully
    /// (structure and file names); a magnet link's metadata is checked once peers send it.
    ///
    /// # Errors
    /// [`Error::InvalidTorrent`] for bad input, [`Error::DuplicateTorrent`] when the same torrent
    /// is already a job, or a database error.
    pub async fn add_torrent(
        &self,
        name: &str,
        category: Option<&str>,
        priority: i64,
        source: TorrentSource<'_>,
    ) -> Result<i64, Error> {
        let (info_hash, magnet, metainfo, total, message) = match source {
            TorrentSource::Magnet(link) => {
                let parsed = fangst_torrent::parse_magnet(link).map_err(Error::InvalidTorrent)?;
                (
                    parsed.info_hash,
                    Some(link),
                    None,
                    0,
                    "added from a magnet link; the file list arrives from peers".to_owned(),
                )
            }
            TorrentSource::File(bytes) => {
                let meta = fangst_torrent::inspect(bytes).map_err(Error::InvalidTorrent)?;
                let message = describe_metadata("added", &meta);
                (meta.info_hash, None, Some(bytes), meta.total_bytes, message)
            }
        };
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT job_id FROM torrent_jobs WHERE info_hash = ?")
                .bind(&info_hash)
                .fetch_optional(&self.pool)
                .await?;
        if let Some(id) = existing {
            return Err(Error::DuplicateTorrent(id));
        }
        let mut tx = self.pool.begin().await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO jobs (kind, name, category, priority, status, total_bytes, added_at)
             VALUES ('torrent', ?, ?, ?, 'queued', ?, ?) RETURNING id",
        )
        .bind(name)
        .bind(category)
        .bind(priority)
        .bind(i64::try_from(total).unwrap_or(i64::MAX))
        .bind(now())
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO torrent_jobs (job_id, info_hash, magnet, metainfo) VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&info_hash)
        .bind(magnet)
        .bind(metainfo)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO job_events (job_id, at, kind, message) VALUES (?, ?, 'added', ?)")
            .bind(id)
            .bind(now())
            .bind(message)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// The v1 info hash of a torrent job, as lowercase hex.
    ///
    /// # Errors
    /// [`Error::NotFound`] when the job is not a torrent job.
    pub async fn torrent_info_hash(&self, id: i64) -> Result<String, Error> {
        sqlx::query_scalar("SELECT info_hash FROM torrent_jobs WHERE job_id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound(id))
    }

    async fn torrent(&self, id: i64) -> Result<TorrentRow, Error> {
        let row: Option<(Option<String>, Option<Vec<u8>>)> =
            sqlx::query_as("SELECT magnet, metainfo FROM torrent_jobs WHERE job_id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let (magnet, metainfo) = row.ok_or(Error::NotFound(id))?;
        Ok(TorrentRow { magnet, metainfo })
    }

    async fn set_torrent_metadata(&self, id: i64, meta: &Metadata) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE torrent_jobs SET metainfo = ? WHERE job_id = ?")
            .bind(&meta.torrent)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE jobs SET total_bytes = ? WHERE id = ?")
            .bind(i64::try_from(meta.total_bytes).unwrap_or(i64::MAX))
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO job_events (job_id, at, kind, message) VALUES (?, ?, 'metadata', ?)",
        )
        .bind(id)
        .bind(now())
        .bind(describe_metadata("metadata received", meta))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn describe_metadata(what: &str, meta: &Metadata) -> String {
    format!(
        "{what} with {} files, {} bytes",
        meta.files.len(),
        meta.total_bytes
    )
}

/// Runs torrent jobs on one engine. Unlike usenet jobs, several run at once, since a torrent's
/// speed depends on its peers and one slow swarm must not hold up the rest.
#[derive(Clone)]
pub struct TorrentRunner {
    pub store: Store,
    pub engine: Engine,
    /// Folder each job gets a subfolder in.
    pub root: PathBuf,
    pub(crate) live: Live,
}

impl TorrentRunner {
    /// Runs one torrent job to the end. A finished torrent is removed from the engine and stops
    /// uploading; the files stay in the job folder.
    ///
    /// # Errors
    /// When the database fails. A torrent that fails is marked failed with the reason.
    pub async fn run(&self, job: &Job) -> Result<(), Error> {
        self.store
            .set_status(job.id, Status::Downloading, None)
            .await?;
        self.execute(job).await
    }

    /// [`TorrentRunner::run`] for a job whose status the caller already set to downloading.
    pub(crate) async fn execute(&self, job: &Job) -> Result<(), Error> {
        let running = self.store.start_meter(job.id);
        let failure = self.download(job, &running.meter).await?;
        running.finish().await?;
        match failure {
            Some(reason) => {
                self.store
                    .set_status(job.id, Status::Failed, Some(&reason))
                    .await
            }
            None => self.store.set_status(job.id, Status::Completed, None).await,
        }
    }

    /// Fetches the metadata if needed and downloads the torrent. Returns why it fails, if it does.
    async fn download(&self, job: &Job, meter: &Meter) -> Result<Option<String>, Error> {
        let out = self.store.job_folder(job, &self.root).await?;
        let row = self.store.torrent(job.id).await?;
        let metainfo = if let Some(bytes) = row.metainfo {
            bytes
        } else {
            let Some(link) = row.magnet else {
                return Ok(Some(
                    "the job has neither metadata nor a magnet link".to_owned(),
                ));
            };
            self.store
                .add_event(job.id, "metadata", "asking peers for the file list")
                .await?;
            match self.engine.resolve(&link).await {
                Ok(meta) => {
                    self.store.set_torrent_metadata(job.id, &meta).await?;
                    meta.torrent
                }
                Err(e) => return Ok(Some(e.to_string())),
            }
        };

        let download = match self.engine.start_torrent(&metainfo, &out).await {
            Ok(download) => download,
            Err(e) => return Ok(Some(e.to_string())),
        };
        let _watching = self.live.watch(job.id, download.watch());
        let kept = download.kept_bytes();
        let total = download.total_bytes();
        meter.set_target(total);
        meter.follow(download.have_bytes_reader());
        if kept > 0 {
            self.store
                .add_event(
                    job.id,
                    "resumed",
                    &format!("resuming: {kept} of {total} bytes already on disk"),
                )
                .await?;
        }
        let result = download.wait().await;
        meter.settle(download.have_bytes());
        download.stop().await;
        match result {
            Ok(()) => {
                self.store
                    .add_event(
                        job.id,
                        "transfer",
                        &format!("downloaded {} bytes from peers", total - kept),
                    )
                    .await?;
                Ok(None)
            }
            Err(e) => Ok(Some(e.to_string())),
        }
    }
}
