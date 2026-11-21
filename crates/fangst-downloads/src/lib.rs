//! The download queue.
//!
//! [`Store`] owns `downloads.db`: jobs for every protocol in one table, protocol details beside it.
//! [`Runner`] takes queued jobs in priority order and drives the protocol engine, recording each
//! stored segment so a restart continues where the last run stopped.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::{FromRow, Row};

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
    /// Unix seconds.
    pub added_at: i64,
    pub finished_at: Option<i64>,
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
    added_at: i64,
    finished_at: Option<i64>,
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
            added_at: row.added_at,
            finished_at: row.finished_at,
        })
    }
}

#[derive(Debug)]
pub enum Error {
    Database(sqlx::Error),
    Migration(String),
    InvalidNzb(fangst_nzb::Error),
    NotFound(i64),
    Corrupt(String),
    Download(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::Migration(e) => write!(f, "could not migrate downloads.db: {e}"),
            Self::InvalidNzb(e) => write!(f, "invalid NZB: {e}"),
            Self::NotFound(id) => write!(f, "job {id} does not exist"),
            Self::Corrupt(e) => write!(f, "downloads.db holds unexpected data: {e}"),
            Self::Download(e) => write!(f, "download failed: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

/// Column list shared by every job query; a macro so the SQL stays a `&'static str`, which sqlx requires.
macro_rules! job_columns {
    () => {
        "id, kind, name, category, priority, status, error, output_path, total_bytes, added_at, finished_at"
    };
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
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
        Ok(Self { pool })
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
            " FROM jobs ORDER BY id DESC"
        ))
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(Job::try_from)
        .collect()
    }

    /// # Errors
    /// [`Error::NotFound`] when the id doesn't exist.
    pub async fn get(&self, id: i64) -> Result<Job, Error> {
        sqlx::query_as::<_, JobRow>(concat!(
            "SELECT ",
            job_columns!(),
            " FROM jobs WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound(id))?
        .try_into()
    }

    /// The queued job that should run next: highest priority, then oldest.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn next_queued(&self) -> Result<Option<Job>, Error> {
        sqlx::query_as::<_, JobRow>(concat!(
            "SELECT ",
            job_columns!(),
            " FROM jobs WHERE status = 'queued' ORDER BY priority DESC, id LIMIT 1"
        ))
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
        sqlx::query("UPDATE jobs SET status = ?, error = ?, finished_at = ? WHERE id = ?")
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
pub struct Runner {
    pub store: Store,
    pub pool: Arc<fangst_nntp::Pool>,
    /// Folder each job gets a subfolder in.
    pub root: PathBuf,
    pub options: fangst_usenet::Options,
}

impl Runner {
    /// Runs jobs until none are queued, then returns.
    ///
    /// # Errors
    /// When the database fails. A job that fails is marked failed and the runner moves on.
    pub async fn run_until_idle(&self) -> Result<(), Error> {
        self.store.requeue_interrupted().await?;
        while let Some(job) = self.store.next_queued().await? {
            self.run(&job).await?;
        }
        Ok(())
    }

    async fn run(&self, job: &Job) -> Result<(), Error> {
        self.store
            .set_status(job.id, Status::Downloading, None)
            .await?;
        let out = if let Some(path) = &job.output_path {
            path.clone()
        } else {
            let folder =
                fangst_core::path::safe_file_name(&job.name).unwrap_or_else(|| "job".to_owned());
            let path = self.root.join(format!("{}-{folder}", job.id));
            self.store.set_output_path(job.id, &path).await?;
            path
        };
        let nzb_bytes = self.store.nzb(job.id).await?;
        let nzb = match fangst_nzb::parse(&nzb_bytes, &fangst_nzb::Limits::default()) {
            Ok(nzb) => nzb,
            Err(e) => {
                self.store
                    .set_status(job.id, Status::Failed, Some(&format!("invalid NZB: {e}")))
                    .await?;
                return Ok(());
            }
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
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(usize, u32)>();
        let store = self.store.clone();
        let job_id = job.id;
        let recorder = tokio::spawn(async move {
            while let Some((file, number)) = rx.recv().await {
                store.mark_segment_done(job_id, file, number).await?;
            }
            Ok::<(), Error>(())
        });
        let resume = fangst_usenet::Resume {
            done,
            on_stored: Arc::new(move |file, number| {
                let _ = tx.send((file, number));
            }),
        };
        let progress: fangst_usenet::ProgressFn = Arc::new(|_| {});
        let result = fangst_usenet::download_resumable(
            &nzb,
            &out,
            Arc::clone(&self.pool),
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
                    .add_event(job.id, "file", &describe_file(file))
                    .await?;
            }
        }
        match result {
            Ok(report) if report.is_complete() => {
                self.store.set_status(job.id, Status::Completed, None).await
            }
            Ok(report) => {
                let missing: usize = report.files.iter().map(|f| f.missing_segments.len()).sum();
                let failed: usize = report.files.iter().map(|f| f.failed_segments.len()).sum();
                let reason = format!(
                    "{missing} segments missing on the server, {failed} could not be downloaded"
                );
                self.store
                    .set_status(job.id, Status::Failed, Some(&reason))
                    .await
            }
            Err(e) => {
                self.store
                    .set_status(job.id, Status::Failed, Some(&e.to_string()))
                    .await
            }
        }
    }
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
