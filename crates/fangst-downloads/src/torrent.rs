//! Torrent jobs: what the store keeps about them and how one runs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fangst_core::defaults::Seeding as SeedingDefaults;
use fangst_torrent::{Download, Engine, Metadata, SeedClock, SeedLimits, Seeded, Watch};

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
#[derive(Debug, Clone, PartialEq)]
pub struct TorrentStatus {
    /// Lowercase hex of the v1 info hash.
    pub info_hash: String,
    /// Empty until the metadata is known, which for a magnet link means until a peer sends it.
    pub files: Vec<TorrentFile>,
    /// Bytes on disk that passed the hash check: live while running, all of them once completed.
    pub have_bytes: u64,
    /// Bytes sent to peers over the job's life: earlier runs as recorded, plus the live one.
    pub uploaded_bytes: u64,
    pub download_bps: u64,
    pub upload_bps: u64,
    /// The job completed and still uploads to peers.
    pub seeding: bool,
    /// Time spent seeding after the download finished, over every run.
    pub seeding_secs: u64,
    /// The limits that apply to this job: its category's, else the defaults. `None` is off.
    pub ratio_limit: Option<f64>,
    pub seeding_time_limit_secs: Option<u64>,
}

/// Seeding limits a category sets. `None` uses the default; a negative value turns that limit off.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CategorySeeding {
    /// Uploaded bytes over the size.
    pub ratio_limit: Option<f64>,
    pub time_limit_secs: Option<i64>,
}

impl CategorySeeding {
    /// These limits over `defaults`: a category's own value wins when set.
    #[must_use]
    pub fn over(&self, defaults: &SeedingDefaults) -> SeedLimits {
        let ratio = self
            .ratio_limit
            .map_or_else(|| defaults.ratio_limit(), |r| (r >= 0.0).then_some(r));
        let time = self.time_limit_secs.map_or_else(
            || defaults.time_limit(),
            |s| u64::try_from(s).ok().map(Duration::from_secs),
        );
        SeedLimits { ratio, time }
    }
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

/// Seeding torrents by job id, so their upload and seed time read live between database writes.
#[derive(Clone, Default)]
pub(crate) struct Seeds(Arc<Mutex<HashMap<i64, SeedClock>>>);

/// Removes its job from [`Seeds`] when dropped, which also covers a seeding task that is aborted.
struct SeedWatching {
    seeds: Seeds,
    id: i64,
}

impl Drop for SeedWatching {
    fn drop(&mut self) {
        self.seeds.0.lock().expect("seeds lock").remove(&self.id);
    }
}

impl Seeds {
    fn watch(&self, id: i64, clock: SeedClock) -> SeedWatching {
        self.0.lock().expect("seeds lock").insert(id, clock);
        SeedWatching {
            seeds: self.clone(),
            id,
        }
    }

    pub(crate) fn clock(&self, id: i64) -> Option<SeedClock> {
        self.0.lock().expect("seeds lock").get(&id).cloned()
    }
}

/// The status of a torrent job, or `None` for an unknown id or a usenet job.
pub(crate) async fn status(
    store: &Store,
    live: Option<&Live>,
    seeding: &SeedingDefaults,
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
        .map_or_default(|meta| {
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
        });
    let total = u64::try_from(job.total_bytes).unwrap_or(0);
    let progress = live.and_then(|live| live.progress(id));
    let limits = store.category_seeding_of(id).await?.over(seeding);
    let clock = store.seeds.clock(id);
    // A downloading run's uploads reach the database only once it seeds; `job` already holds a
    // seeding run's.
    let unrecorded = if clock.is_none() {
        progress.map_or(0, |p| p.uploaded_bytes)
    } else {
        0
    };
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
        uploaded_bytes: u64::try_from(job.uploaded_bytes).unwrap_or(0) + unrecorded,
        download_bps: progress.map_or(0, |p| p.download_bps),
        upload_bps: clock.map_or_else(|| progress.map_or(0, |p| p.upload_bps), |c| c.upload_bps()),
        seeding: job.seeding,
        seeding_secs: u64::try_from(job.seeding_secs).unwrap_or(0),
        ratio_limit: limits.ratio,
        seeding_time_limit_secs: limits.time.map(|t| t.as_secs()),
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

    /// What earlier runs recorded for a torrent job's seeding.
    async fn seeded(&self, id: i64) -> Result<Seeded, Error> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT uploaded_bytes, seeding_secs FROM torrent_jobs WHERE job_id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let (uploaded, secs) = row.ok_or(Error::NotFound(id))?;
        Ok(Seeded {
            uploaded_bytes: u64::try_from(uploaded).unwrap_or(0),
            time: Duration::from_secs(u64::try_from(secs).unwrap_or(0)),
        })
    }

    async fn record_seeded(&self, id: i64, seeded: Seeded) -> Result<(), Error> {
        sqlx::query(
            "UPDATE torrent_jobs SET uploaded_bytes = ?, seeding_secs = ? WHERE job_id = ?",
        )
        .bind(i64::try_from(seeded.uploaded_bytes).unwrap_or(i64::MAX))
        .bind(i64::try_from(seeded.time.as_secs()).unwrap_or(i64::MAX))
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Records the counts and whether the job seeds, with a timeline entry saying why.
    async fn set_seeding(
        &self,
        id: i64,
        seeding: bool,
        seeded: Seeded,
        message: &str,
    ) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            "UPDATE torrent_jobs SET seeding = ?, uploaded_bytes = ?, seeding_secs = ? WHERE job_id = ?",
        )
        .bind(seeding)
        .bind(i64::try_from(seeded.uploaded_bytes).unwrap_or(i64::MAX))
        .bind(i64::try_from(seeded.time.as_secs()).unwrap_or(i64::MAX))
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            return Err(Error::NotFound(id));
        }
        sqlx::query(
            "INSERT INTO job_events (job_id, at, kind, message) VALUES (?, ?, 'seeding', ?)",
        )
        .bind(id)
        .bind(now())
        .bind(message)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Completed torrent jobs that were still seeding when the last run stopped.
    pub(crate) async fn seeding_jobs(&self) -> Result<Vec<Job>, Error> {
        sqlx::query_as::<_, crate::JobRow>(concat!(
            "SELECT ",
            job_columns!(),
            " FROM ",
            job_from!(),
            " WHERE kind = 'torrent' AND status = 'completed' AND seeding = 1 ORDER BY jobs.id"
        ))
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(Job::try_from)
        .collect()
    }
}

fn describe_limits(limits: &SeedLimits) -> String {
    match (limits.ratio, limits.time) {
        (None, None) => "with no limit".to_owned(),
        (Some(ratio), None) => format!("until ratio {ratio:.2}"),
        (None, Some(time)) => format!("for {} seconds", time.as_secs()),
        (Some(ratio), Some(time)) => format!(
            "until ratio {ratio:.2} or for {} seconds, whichever comes first",
            time.as_secs()
        ),
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
    /// Limits for jobs whose category sets none.
    pub(crate) seeding: SeedingDefaults,
    /// How often a seeding job's counts are written and its limits read again.
    pub(crate) persist_interval: Duration,
    /// Where seeding starting and stopping is announced.
    pub(crate) changes: tokio::sync::broadcast::Sender<crate::Change>,
}

impl TorrentRunner {
    /// Downloads a job whose status the caller already set to downloading. Returns the finished
    /// torrent, still in the engine and already marked seeding, for the caller to [`Self::seed`].
    /// A torrent that fails is marked failed with the reason.
    pub(crate) async fn execute(&self, job: &Job) -> Result<Option<Download>, Error> {
        let running = self.store.start_meter(job.id);
        let outcome = self.download(job, &running.meter).await?;
        running.finish().await?;
        match outcome {
            Err(reason) => {
                self.store
                    .set_status(job.id, Status::Failed, Some(&reason))
                    .await?;
                Ok(None)
            }
            Ok(download) => {
                // Marked first, so no reader sees a completed job that looks done seeding: a
                // client could remove it before seeding starts.
                sqlx::query("UPDATE torrent_jobs SET seeding = 1 WHERE job_id = ?")
                    .bind(job.id)
                    .execute(&self.store.pool)
                    .await?;
                self.store
                    .set_status(job.id, Status::Completed, None)
                    .await?;
                Ok(Some(download))
            }
        }
    }

    /// Seeds a completed job until a limit is reached, then removes it from the engine and keeps
    /// the files. Counts are written every persist interval and limits read again then, so a
    /// category changed meanwhile applies. `resumed` marks a restart picking seeding back up.
    pub(crate) async fn seed(
        &self,
        id: i64,
        download: Download,
        resumed: bool,
    ) -> Result<(), Error> {
        let earlier = self.store.seeded(id).await?;
        let clock = download.seed_clock(earlier);
        let watching = self.store.seeds.watch(id, clock.clone());
        let mut limits = self
            .store
            .category_seeding_of(id)
            .await?
            .over(&self.seeding);
        let message = if resumed {
            format!(
                "seeding again {}, {} seconds already counted",
                describe_limits(&limits),
                earlier.time.as_secs()
            )
        } else {
            format!("seeding {}", describe_limits(&limits))
        };
        self.store
            .set_seeding(id, true, clock.seeded(), &message)
            .await?;
        let _ = self.changes.send(crate::Change::Updated(id));
        let every = self.persist_interval.max(Duration::from_millis(1));
        let stop = loop {
            if let Ok(stop) = tokio::time::timeout(every, download.seed(&clock, &limits)).await {
                break stop;
            }
            self.store.record_seeded(id, clock.seeded()).await?;
            limits = self
                .store
                .category_seeding_of(id)
                .await?
                .over(&self.seeding);
        };
        let seeded = clock.seeded();
        self.store
            .set_seeding(
                id,
                false,
                seeded,
                &format!(
                    "stopped seeding: {stop}; uploaded {} bytes in {} seconds of seeding",
                    seeded.uploaded_bytes,
                    seeded.time.as_secs()
                ),
            )
            .await?;
        drop(watching);
        download.stop().await;
        let _ = self.changes.send(crate::Change::Updated(id));
        Ok(())
    }

    /// Puts a job that was seeding when the daemon stopped back in the engine. Its files are
    /// checked first; when they no longer pass, seeding ends with the reason.
    pub(crate) async fn reseed(&self, job: &Job) -> Result<(), Error> {
        let earlier = self.store.seeded(job.id).await?;
        let row = self.store.torrent(job.id).await?;
        let (Some(metainfo), Some(folder)) = (row.metainfo, job.output_path.as_ref()) else {
            return self
                .store
                .set_seeding(
                    job.id,
                    false,
                    earlier,
                    "stopped seeding: the job has no metadata or folder",
                )
                .await;
        };
        let reason = match self.engine.start_torrent(&metainfo, folder).await {
            Ok(download) if download.kept_bytes() == download.total_bytes() => {
                return self.seed(job.id, download, true).await;
            }
            Ok(download) => {
                let reason = format!(
                    "only {} of {} bytes on disk pass the hash check",
                    download.kept_bytes(),
                    download.total_bytes()
                );
                download.stop().await;
                reason
            }
            Err(e) => e.to_string(),
        };
        self.store
            .set_seeding(
                job.id,
                false,
                earlier,
                &format!("stopped seeding: {reason}"),
            )
            .await
    }

    /// Fetches the metadata if needed and downloads the torrent. Returns the finished torrent, or
    /// why it failed.
    async fn download(&self, job: &Job, meter: &Meter) -> Result<Result<Download, String>, Error> {
        let out = self.store.job_folder(job, &self.root).await?;
        let row = self.store.torrent(job.id).await?;
        let metainfo = if let Some(bytes) = row.metainfo {
            bytes
        } else {
            let Some(link) = row.magnet else {
                return Ok(Err(
                    "the job has neither metadata nor a magnet link".to_owned()
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
                Err(e) => return Ok(Err(e.to_string())),
            }
        };

        let download = match self.engine.start_torrent(&metainfo, &out).await {
            Ok(download) => download,
            Err(e) => return Ok(Err(e.to_string())),
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
        match result {
            Ok(()) => {
                self.store
                    .add_event(
                        job.id,
                        "transfer",
                        &format!("downloaded {} bytes from peers", total - kept),
                    )
                    .await?;
                Ok(Ok(download))
            }
            Err(e) => {
                download.stop().await;
                Ok(Err(e.to_string()))
            }
        }
    }
}
