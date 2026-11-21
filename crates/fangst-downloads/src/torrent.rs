//! Torrent jobs: what the store keeps about them and how one runs.

use std::path::PathBuf;

use fangst_torrent::{Engine, Metadata};

use crate::{Error, Job, Status, Store, now};

/// What a user hands in to start a torrent job.
#[derive(Debug, Clone, Copy)]
pub enum TorrentSource<'a> {
    Magnet(&'a str),
    /// The bytes of a `.torrent` file.
    File(&'a [u8]),
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
        let out = self.store.job_folder(job, &self.root).await?;
        let row = self.store.torrent(job.id).await?;
        let metainfo = if let Some(bytes) = row.metainfo {
            bytes
        } else {
            let Some(link) = row.magnet else {
                return self
                    .fail(job.id, "the job has neither metadata nor a magnet link")
                    .await;
            };
            self.store
                .add_event(job.id, "metadata", "asking peers for the file list")
                .await?;
            match self.engine.resolve(&link).await {
                Ok(meta) => {
                    self.store.set_torrent_metadata(job.id, &meta).await?;
                    meta.torrent
                }
                Err(e) => return self.fail(job.id, &e.to_string()).await,
            }
        };

        let download = match self.engine.start_torrent(&metainfo, &out).await {
            Ok(download) => download,
            Err(e) => return self.fail(job.id, &e.to_string()).await,
        };
        let kept = download.kept_bytes();
        let total = download.total_bytes();
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
                self.store.set_status(job.id, Status::Completed, None).await
            }
            Err(e) => self.fail(job.id, &e.to_string()).await,
        }
    }

    async fn fail(&self, id: i64, reason: &str) -> Result<(), Error> {
        self.store
            .set_status(id, Status::Failed, Some(reason))
            .await
    }
}
