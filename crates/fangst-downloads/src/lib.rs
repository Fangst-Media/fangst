//! The download queue.
//!
//! [`Store`] owns `downloads.db`: jobs for every protocol in one table, protocol details beside it.
//! [`Runner`] takes queued jobs in priority order and drives the protocol engine, recording each
//! stored segment so a restart continues where the last run stopped.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::{FromRow, Row};

/// Column list shared by every job query; a macro so the SQL stays a `&'static str`, which sqlx
/// requires. Defined before the modules so they can use it too.
macro_rules! job_columns {
    () => {
        "jobs.id AS id, kind, name, category, priority, status, error, output_path, total_bytes, downloaded_bytes, added_at, finished_at, \
         COALESCE(torrent_jobs.uploaded_bytes, 0) AS uploaded_bytes, COALESCE(torrent_jobs.seeding_secs, 0) AS seeding_secs, \
         COALESCE(torrent_jobs.seeding, 0) AS seeding"
    };
}

/// The tables [`job_columns`] reads from.
macro_rules! job_from {
    () => {
        "jobs LEFT JOIN torrent_jobs ON torrent_jobs.job_id = jobs.id"
    };
}

mod auth;
mod categories;
mod notify;
mod pipeline;
mod progress;
mod service;
mod torrent;
pub use auth::{Auth, ENVIRONMENT_KEY, FIRST_USER, MIN_PASSWORD_CHARS, Secrets, User};
pub use notify::Notifications;
use progress::Meter;
pub use progress::Progress;
pub use service::{Change, Service};
pub use torrent::{CategorySeeding, TorrentFile, TorrentRunner, TorrentSource, TorrentStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Usenet,
    Torrent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
}

macro_rules! text_enum {
    ($ty:ty { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl $ty {
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }
        impl FromStr for $ty {
            type Err = Error;
            fn from_str(s: &str) -> Result<Self, Error> {
                match s {
                    $($text => Ok(Self::$variant),)+
                    other => Err(Error::Corrupt(format!("unknown {} `{other}`", stringify!($ty)))),
                }
            }
        }
    };
}

text_enum!(Kind { Usenet => "usenet", Torrent => "torrent" });
text_enum!(Status {
    Queued => "queued",
    Downloading => "downloading",
    Paused => "paused",
    Completed => "completed",
    Failed => "failed",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: i64,
    pub kind: Kind,
    pub name: String,
    pub category: Option<String>,
    pub priority: i64,
    pub status: Status,
    /// Why the job failed, in words a user can act on.
    pub error: Option<String>,
    pub output_path: Option<PathBuf>,
    pub total_bytes: i64,
    /// Bytes on disk: live while the job runs in this process, else as last recorded. A completed
    /// job counts as `total_bytes`, even when PAR2 recovery volumes it did not need were skipped.
    pub downloaded_bytes: i64,
    /// Averaged over the last few seconds; 0 unless the job is downloading.
    pub rate_bytes_per_sec: i64,
    /// Seconds left at the current rate; `None` when not downloading or not yet known.
    pub eta_seconds: Option<i64>,
    /// Unix seconds.
    pub added_at: i64,
    pub finished_at: Option<i64>,
    /// A completed torrent job that still uploads to peers. Usenet jobs never seed.
    pub seeding: bool,
    /// Bytes a torrent job sent to peers over every run; live while it seeds.
    pub uploaded_bytes: i64,
    /// Seconds a torrent job seeded after its download finished, over every run; live while it seeds.
    pub seeding_secs: i64,
}

/// One entry in a job's timeline.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Event {
    /// Unix seconds.
    pub at: i64,
    /// Stable machine name: `added`, `status`, `resumed`, `file`, ...
    pub kind: String,
    /// What happened, in words a user can act on.
    pub message: String,
}

#[derive(FromRow)]
struct JobRow {
    id: i64,
    kind: String,
    name: String,
    category: Option<String>,
    priority: i64,
    status: String,
    error: Option<String>,
    output_path: Option<String>,
    total_bytes: i64,
    downloaded_bytes: i64,
    added_at: i64,
    finished_at: Option<i64>,
    uploaded_bytes: i64,
    seeding_secs: i64,
    seeding: bool,
}

impl TryFrom<JobRow> for Job {
    type Error = Error;
    fn try_from(row: JobRow) -> Result<Self, Error> {
        Ok(Self {
            id: row.id,
            kind: row.kind.parse()?,
            name: row.name,
            category: row.category,
            priority: row.priority,
            status: row.status.parse()?,
            error: row.error,
            output_path: row.output_path.map(PathBuf::from),
            total_bytes: row.total_bytes,
            downloaded_bytes: row.downloaded_bytes,
            rate_bytes_per_sec: 0,
            eta_seconds: None,
            added_at: row.added_at,
            finished_at: row.finished_at,
            seeding: row.seeding,
            uploaded_bytes: row.uploaded_bytes,
            seeding_secs: row.seeding_secs,
        })
    }
}

#[derive(Debug)]
pub enum Error {
    Database(sqlx::Error),
    Migration(String),
    InvalidNzb(fangst_nzb::Error),
    InvalidTorrent(fangst_torrent::Error),
    /// The torrent is already job `0`.
    DuplicateTorrent(i64),
    NotFound(i64),
    Corrupt(String),
    Download(String),
    InvalidState {
        id: i64,
        status: Status,
        action: &'static str,
    },
    /// Input a caller can fix, in words they can act on.
    Invalid(String),
    Secret(fangst_core::secret::Error),
    /// Every password-hashing slot is in use; try again shortly.
    Busy,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::Migration(e) => write!(f, "could not migrate downloads.db: {e}"),
            Self::InvalidNzb(e) => write!(f, "invalid NZB: {e}"),
            Self::InvalidTorrent(e) => write!(f, "{e}"),
            Self::DuplicateTorrent(id) => write!(f, "this torrent is already job {id}"),
            Self::NotFound(id) => write!(f, "job {id} does not exist"),
            Self::Corrupt(e) => write!(f, "downloads.db holds unexpected data: {e}"),
            Self::Download(e) => write!(f, "download failed: {e}"),
            Self::InvalidState { id, status, action } => {
                write!(f, "job {id} is {} and can't be {action}d", status.as_str())
            }
            Self::Invalid(e) => f.write_str(e),
            Self::Secret(e) => e.fmt(f),
            Self::Busy => f.write_str("too many password checks at once; try again in a moment"),
        }
    }
}

impl std::error::Error for Error {}

impl From<fangst_core::secret::Error> for Error {
    fn from(e: fangst_core::secret::Error) -> Self {
        Self::Secret(e)
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[derive(Clone)]
pub struct Store {
    pub(crate) pool: SqlitePool,
    pub(crate) meters: progress::Meters,
    pub(crate) seeds: torrent::Seeds,
}

impl Store {
    /// Opens or creates the database, backing it up before any migration touches it.
    ///
    /// # Errors
    /// When the file can't be opened, backed up or migrated.
    pub async fn open(path: &Path) -> Result<Self, Error> {
        let existed = path.exists();
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            // NORMAL in WAL mode can lose the last transactions on power loss but never corrupts;
            // a lost segment record only means downloading that segment again.
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;

        let migrator = sqlx::migrate!("./migrations");
        if existed {
            let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations")
                .fetch_all(&pool)
                .await
                .unwrap_or_default();
            if let Some(pending) = migrator
                .iter()
                .map(|m| m.version)
                .filter(|v| !applied.contains(v))
                .max()
            {
                backup(&pool, path, pending).await?;
            }
        }
        migrator
            .run(&pool)
            .await
            .map_err(|e| Error::Migration(e.to_string()))?;
        Ok(Self {
            pool,
            meters: progress::Meters::default(),
            seeds: torrent::Seeds::default(),
        })
    }

    /// Queues an NZB after checking that it parses.
    ///
    /// # Errors
    /// When the NZB is invalid or the database write fails.
    pub async fn add_nzb(
        &self,
        name: &str,
        category: Option<&str>,
        priority: i64,
        nzb: &[u8],
    ) -> Result<i64, Error> {
        let parsed =
            fangst_nzb::parse(nzb, &fangst_nzb::Limits::default()).map_err(Error::InvalidNzb)?;
        let total = i64::try_from(parsed.total_bytes()).unwrap_or(i64::MAX);
        let mut tx = self.pool.begin().await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO jobs (kind, name, category, priority, status, total_bytes, added_at)
             VALUES ('usenet', ?, ?, ?, 'queued', ?, ?) RETURNING id",
        )
        .bind(name)
        .bind(category)
        .bind(priority)
        .bind(total)
        .bind(now())
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO usenet_jobs (job_id, nzb) VALUES (?, ?)")
            .bind(id)
            .bind(nzb)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO job_events (job_id, at, kind, message) VALUES (?, ?, 'added', ?)")
            .bind(id)
            .bind(now())
            .bind(format!(
                "added with {} files, {} bytes",
                parsed.files.len(),
                parsed.total_bytes()
            ))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Appends an entry to a job's timeline.
    ///
    /// # Errors
    /// When the database write fails.
    pub async fn add_event(&self, id: i64, kind: &str, message: &str) -> Result<(), Error> {
        sqlx::query("INSERT INTO job_events (job_id, at, kind, message) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(now())
            .bind(kind)
            .bind(message)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// A job's timeline, oldest first.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn timeline(&self, id: i64) -> Result<Vec<Event>, Error> {
        Ok(sqlx::query_as::<_, Event>(
            "SELECT at, kind, message FROM job_events WHERE job_id = ? ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Every job, newest first.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn list(&self) -> Result<Vec<Job>, Error> {
        sqlx::query_as::<_, JobRow>(concat!(
            "SELECT ",
            job_columns!(),
            " FROM ",
            job_from!(),
            " ORDER BY jobs.id DESC"
        ))
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| Job::try_from(row).map(|job| self.with_live_progress(job)))
        .collect()
    }

    /// # Errors
    /// [`Error::NotFound`] when the id doesn't exist.
    pub async fn get(&self, id: i64) -> Result<Job, Error> {
        sqlx::query_as::<_, JobRow>(concat!(
            "SELECT ",
            job_columns!(),
            " FROM ",
            job_from!(),
            " WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound(id))?
        .try_into()
        .map(|job| self.with_live_progress(job))
    }

    /// Replaces the recorded counts of a job this process is downloading or seeding with live ones.
    fn with_live_progress(&self, mut job: Job) -> Job {
        if job.seeding
            && let Some(clock) = self.seeds.clock(job.id)
        {
            let seeded = clock.seeded();
            job.uploaded_bytes = i64::try_from(seeded.uploaded_bytes).unwrap_or(i64::MAX);
            job.seeding_secs = i64::try_from(seeded.time.as_secs()).unwrap_or(i64::MAX);
        }
        if job.status == Status::Downloading
            && let Some(live) = self.progress(job.id)
        {
            job.downloaded_bytes = i64::try_from(live.downloaded_bytes).unwrap_or(i64::MAX);
            job.rate_bytes_per_sec = i64::try_from(live.rate_bytes_per_sec).unwrap_or(i64::MAX);
            job.eta_seconds = live
                .eta_seconds
                .map(|s| i64::try_from(s).unwrap_or(i64::MAX));
        }
        job
    }

    /// The queued job that should run next: highest priority, then oldest.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn next_queued(&self) -> Result<Option<Job>, Error> {
        sqlx::query_as::<_, JobRow>(concat!(
            "SELECT ",
            job_columns!(),
            " FROM ",
            job_from!(),
            " WHERE status = 'queued' ORDER BY priority DESC, id LIMIT 1"
        ))
        .fetch_optional(&self.pool)
        .await?
        .map(Job::try_from)
        .transpose()
    }

    /// Like [`Store::next_queued`], limited to one protocol, so each protocol's lane only picks its own jobs.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn next_queued_of(&self, kind: Kind) -> Result<Option<Job>, Error> {
        sqlx::query_as::<_, JobRow>(concat!(
            "SELECT ",
            job_columns!(),
            " FROM ",
            job_from!(),
            " WHERE status = 'queued' AND kind = ? ORDER BY priority DESC, id LIMIT 1"
        ))
        .bind(kind.as_str())
        .fetch_optional(&self.pool)
        .await?
        .map(Job::try_from)
        .transpose()
    }

    /// # Errors
    /// When the database write fails.
    pub async fn set_status(
        &self,
        id: i64,
        status: Status,
        error: Option<&str>,
    ) -> Result<(), Error> {
        let finished = matches!(status, Status::Completed | Status::Failed).then(now);
        let message = match error {
            Some(reason) => format!("{}: {reason}", status.as_str()),
            None => status.as_str().to_owned(),
        };
        // The status change and its timeline entry land together or not at all.
        let mut tx = self.pool.begin().await?;
        // A completed job has everything it needs, even when unneeded PAR2 volumes were skipped.
        sqlx::query(
            "UPDATE jobs SET status = ?1, error = ?2, finished_at = ?3,
             downloaded_bytes = CASE WHEN ?1 = 'completed' THEN total_bytes ELSE downloaded_bytes END
             WHERE id = ?4",
        )
        .bind(status.as_str())
        .bind(error)
        .bind(finished)
        .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO job_events (job_id, at, kind, message) VALUES (?, ?, 'status', ?)",
        )
        .bind(id)
        .bind(now())
        .bind(message)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Removes a job and everything recorded about it.
    ///
    /// # Errors
    /// [`Error::NotFound`] when the id doesn't exist.
    pub async fn delete(&self, id: i64) -> Result<(), Error> {
        let result = sqlx::query("DELETE FROM jobs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(Error::NotFound(id));
        }
        Ok(())
    }

    /// Jobs left `downloading` by a process that stopped go back to the queue.
    ///
    /// # Errors
    /// When the database write fails.
    pub async fn requeue_interrupted(&self) -> Result<u64, Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO job_events (job_id, at, kind, message)
             SELECT id, ?, 'status', 'queued: the previous run stopped before this job finished'
             FROM jobs WHERE status = 'downloading'",
        )
        .bind(now())
        .execute(&mut *tx)
        .await?;
        let result = sqlx::query("UPDATE jobs SET status = 'queued' WHERE status = 'downloading'")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }

    /// The job's folder under `root`, chosen and recorded on its first run.
    async fn job_folder(&self, job: &Job, root: &Path) -> Result<PathBuf, Error> {
        if let Some(path) = &job.output_path {
            return Ok(path.clone());
        }
        let folder =
            fangst_core::path::safe_file_name(&job.name).unwrap_or_else(|| "job".to_owned());
        let path = root.join(format!("{}-{folder}", job.id));
        self.set_output_path(job.id, &path).await?;
        Ok(path)
    }

    async fn set_output_path(&self, id: i64, path: &Path) -> Result<(), Error> {
        sqlx::query("UPDATE jobs SET output_path = ? WHERE id = ?")
            .bind(path.to_string_lossy())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn nzb(&self, id: i64) -> Result<Vec<u8>, Error> {
        sqlx::query_scalar("SELECT nzb FROM usenet_jobs WHERE job_id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound(id))
    }

    /// `(file index, segment number)` pairs already on disk for a usenet job.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn done_segments(&self, id: i64) -> Result<HashSet<(usize, u32)>, Error> {
        let rows =
            sqlx::query("SELECT file_index, number FROM usenet_segments_done WHERE job_id = ?")
                .bind(id)
                .fetch_all(&self.pool)
                .await?;
        rows.iter()
            .map(|row| {
                let file: i64 = row.get(0);
                let number: i64 = row.get(1);
                Ok((
                    usize::try_from(file)
                        .map_err(|_| Error::Corrupt("negative file index".to_owned()))?,
                    u32::try_from(number)
                        .map_err(|_| Error::Corrupt("bad segment number".to_owned()))?,
                ))
            })
            .collect()
    }

    async fn mark_segment_done(&self, id: i64, file: usize, number: u32) -> Result<(), Error> {
        sqlx::query("INSERT OR IGNORE INTO usenet_segments_done (job_id, file_index, number) VALUES (?, ?, ?)")
            .bind(id)
            .bind(i64::try_from(file).unwrap_or(i64::MAX))
            .bind(i64::from(number))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

async fn backup(pool: &SqlitePool, path: &Path, pending: i64) -> Result<(), Error> {
    let target = path.with_extension(format!("db.before-{pending}.bak"));
    let _ = tokio::fs::remove_file(&target).await;
    // VACUUM INTO writes a consistent copy even while WAL holds uncheckpointed pages.
    sqlx::query("VACUUM INTO ?")
        .bind(target.to_string_lossy())
        .execute(pool)
        .await?;
    Ok(())
}

/// Runs queued jobs one at a time.
#[derive(Clone)]
pub struct Runner {
    pub store: Store,
    /// Usenet servers in priority order; backfill servers only get what every primary lacks.
    pub servers: Vec<fangst_usenet::Server>,
    /// Folder each job gets a subfolder in.
    pub root: PathBuf,
    pub options: fangst_usenet::Options,
    /// Run in order after a download completes and verifies; the first `Fail` fails the job.
    pub post_processors: Vec<Arc<dyn fangst_providers::PostProcessor>>,
}

impl Runner {
    /// Runs jobs until none are queued, then returns.
    ///
    /// # Errors
    /// When the database fails. A job that fails is marked failed and the runner moves on.
    pub async fn run_until_idle(&self) -> Result<(), Error> {
        self.store.requeue_interrupted().await?;
        while let Some(job) = self.store.next_queued_of(Kind::Usenet).await? {
            self.run(&job).await?;
        }
        Ok(())
    }

    pub(crate) async fn run(&self, job: &Job) -> Result<(), Error> {
        self.store
            .set_status(job.id, Status::Downloading, None)
            .await?;
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

    /// Downloads, verifies and post-processes a job. Returns why it fails, if it does.
    async fn download(&self, job: &Job, meter: &Arc<Meter>) -> Result<Option<String>, Error> {
        let out = self.store.job_folder(job, &self.root).await?;
        let nzb_bytes = self.store.nzb(job.id).await?;
        let nzb = match fangst_nzb::parse(&nzb_bytes, &fangst_nzb::Limits::default()) {
            Ok(nzb) => nzb,
            Err(e) => return Ok(Some(format!("invalid NZB: {e}"))),
        };

        let done = self.store.done_segments(job.id).await?;
        if !done.is_empty() {
            let total: usize = nzb.files.iter().map(|f| f.segments.len()).sum();
            self.store
                .add_event(
                    job.id,
                    "resumed",
                    &format!(
                        "resuming: {} of {total} segments already on disk",
                        done.len()
                    ),
                )
                .await?;
        }
        let names = fangst_usenet::file_names(&nzb);
        let sets = fangst_postproc_par2::sets(names.iter().map(String::as_str));
        let held = pipeline::held_volumes(&names, &sets);
        let first: HashSet<usize> = (0..names.len()).filter(|i| !held.contains(i)).collect();
        let report = match self.fetch(job.id, &nzb, &out, &first, meter).await? {
            Ok(report) => report,
            Err(e) => return Ok(Some(e.to_string())),
        };
        match self
            .verify_and_repair(job.id, &nzb, &out, &names, &sets, &report, meter)
            .await?
        {
            Some(reason) => Ok(Some(reason)),
            None => self.post_process(job, &nzb, &out).await,
        }
    }

    /// Downloads the `selected` files of `nzb`, recording each stored segment and each file's
    /// result, and counting their bytes on `meter`.
    async fn fetch(
        &self,
        job_id: i64,
        nzb: &fangst_nzb::Nzb,
        out: &Path,
        selected: &HashSet<usize>,
        meter: &Arc<Meter>,
    ) -> Result<Result<fangst_usenet::Report, fangst_usenet::Error>, Error> {
        let done = self.store.done_segments(job_id).await?;
        // Progress counts the segment sizes the NZB lists, the same sizes `total_bytes` adds up,
        // so a job ends at its total.
        let sizes: HashMap<(usize, u32), u64> = nzb
            .files
            .iter()
            .enumerate()
            .filter(|(i, _)| selected.contains(i))
            .flat_map(|(i, file)| file.segments.iter().map(move |s| ((i, s.number), s.bytes)))
            .collect();
        meter.add_target(sizes.values().sum());
        meter.add(
            sizes
                .iter()
                .filter(|(key, _)| done.contains(key))
                .map(|(_, bytes)| bytes)
                .sum(),
        );
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(usize, u32)>();
        let store = self.store.clone();
        let recorder = tokio::spawn(async move {
            while let Some((file, number)) = rx.recv().await {
                store.mark_segment_done(job_id, file, number).await?;
            }
            Ok::<(), Error>(())
        });
        let counted = Arc::clone(meter);
        let resume = fangst_usenet::Resume {
            done,
            on_stored: Arc::new(move |file, number| {
                counted.add(sizes.get(&(file, number)).copied().unwrap_or(0));
                let _ = tx.send((file, number));
            }),
        };
        let progress: fangst_usenet::ProgressFn = Arc::new(|_| {});
        let result = fangst_usenet::download_selected(
            nzb,
            selected,
            out,
            self.servers.clone(),
            self.options,
            progress,
            resume,
        )
        .await;
        recorder
            .await
            .map_err(|e| Error::Download(e.to_string()))??;

        if let Ok(report) = &result {
            for file in &report.files {
                self.store
                    .add_event(job_id, "file", &describe_file(file))
                    .await?;
            }
            for server in report.servers.iter().filter(|s| s.served + s.missed > 0) {
                self.store
                    .add_event(job_id, "server", &describe_server(server))
                    .await?;
            }
        }
        Ok(result)
    }
}

fn describe_server(server: &fangst_usenet::ServerReport) -> String {
    let role = if server.backfill { "backfill " } else { "" };
    format!(
        "{role}server {}: served {} segments, missing {}",
        server.name, server.served, server.missed
    )
}

fn describe_file(file: &fangst_usenet::FileReport) -> String {
    use std::fmt::Write as _;
    let mut text = format!(
        "{}: {} of {} segments stored",
        file.name, file.segments_done, file.segments_total
    );
    if !file.missing_segments.is_empty() {
        let _ = write!(
            text,
            ", {} missing on the server (430)",
            file.missing_segments.len()
        );
    }
    if let Some((number, reason)) = file.failed_segments.first() {
        let _ = write!(
            text,
            ", {} failed (first: segment {number}, {reason})",
            file.failed_segments.len()
        );
    }
    text
}
