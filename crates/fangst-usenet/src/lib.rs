//! Downloads the files an NZB describes.
//!
//! Segments from every file go into one queue that a fixed number of workers drain, each worker
//! holding one pooled connection and keeping several requests in flight. Decoded parts are written
//! at their offset, so they can arrive in any order.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use fangst_nntp::Pool;
use fangst_nzb::Nzb;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Workers, each holding one connection. Should not exceed the pool's connection count.
    pub connections: usize,
    /// Requests each worker keeps in flight.
    pub pipeline_depth: usize,
    /// How often a segment is retried after a connection error before it counts as failed.
    pub retries: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub file_index: usize,
    pub file_name: String,
    pub done_segments: usize,
    pub total_segments: usize,
    pub bytes_written: u64,
}

impl Progress {
    #[must_use]
    pub fn file_finished(&self) -> bool {
        self.done_segments == self.total_segments
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReport {
    pub name: String,
    pub path: PathBuf,
    /// Size from the yEnc header, 0 if no segment arrived.
    pub size: u64,
    pub bytes_written: u64,
    /// Segments every server answered 430 for.
    pub missing_segments: Vec<u32>,
    /// Segments that arrived damaged or could not be fetched, with the reason.
    pub failed_segments: Vec<(u32, String)>,
}

impl FileReport {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.missing_segments.is_empty()
            && self.failed_segments.is_empty()
            && self.bytes_written == self.size
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub files: Vec<FileReport>,
}

impl Report {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.files.iter().all(FileReport::is_complete)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    OutputFolder(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputFolder(msg) => write!(f, "could not use the output folder: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

pub type ProgressFn = Arc<dyn Fn(&Progress) + Send + Sync>;

#[derive(Clone, Copy)]
struct Job {
    file: usize,
    segment: usize,
    attempts: u32,
}

struct FileState {
    name: String,
    path: PathBuf,
    segments: Vec<fangst_nzb::Segment>,
    handle: tokio::sync::Mutex<Option<tokio::fs::File>>,
    size: AtomicU64,
    written: AtomicU64,
    done: AtomicUsize,
    missing: Mutex<Vec<u32>>,
    failed: Mutex<Vec<(u32, String)>>,
}

struct Shared {
    files: Vec<FileState>,
    queue: Mutex<VecDeque<Job>>,
    pool: Arc<Pool>,
    options: Options,
    progress: ProgressFn,
}

/// Downloads every file in `nzb` into `out_dir` and reports what arrived.
///
/// # Errors
/// Only when `out_dir` can't be created. Missing or damaged segments are reported, not errors.
///
/// # Panics
/// If an internal lock was poisoned by a panic in another worker.
pub async fn download(
    nzb: &Nzb,
    out_dir: &Path,
    pool: Arc<Pool>,
    options: Options,
    progress: ProgressFn,
) -> Result<Report, Error> {
    tokio::fs::create_dir_all(out_dir)
        .await
        .map_err(|e| Error::OutputFolder(e.to_string()))?;

    let mut used_names = HashSet::new();
    let files: Vec<FileState> = nzb
        .files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let base = file
                .file_name()
                .and_then(|n| fangst_core::path::safe_file_name(&n))
                .unwrap_or_else(|| format!("file{}.bin", i + 1));
            let name = unique_name(&base, &mut used_names);
            FileState {
                path: out_dir.join(&name),
                name,
                segments: file.segments.clone(),
                handle: tokio::sync::Mutex::new(None),
                size: AtomicU64::new(0),
                written: AtomicU64::new(0),
                done: AtomicUsize::new(0),
                missing: Mutex::new(Vec::new()),
                failed: Mutex::new(Vec::new()),
            }
        })
        .collect();

    let queue = files
        .iter()
        .enumerate()
        .flat_map(|(f, state)| {
            (0..state.segments.len()).map(move |s| Job {
                file: f,
                segment: s,
                attempts: 0,
            })
        })
        .collect();
    let shared = Arc::new(Shared {
        files,
        queue: Mutex::new(queue),
        pool,
        options,
        progress,
    });

    let mut workers = tokio::task::JoinSet::new();
    for _ in 0..options.connections.max(1) {
        workers.spawn(worker(Arc::clone(&shared)));
    }
    while workers.join_next().await.is_some() {}

    let shared = Arc::into_inner(shared).expect("all workers finished");
    let mut reports = Vec::new();
    for state in shared.files {
        if let Some(mut file) = state.handle.into_inner() {
            let _ = file.flush().await;
        }
        let mut missing = state.missing.into_inner().expect("lock");
        missing.sort_unstable();
        let mut failed = state.failed.into_inner().expect("lock");
        failed.sort_unstable();
        reports.push(FileReport {
            name: state.name,
            path: state.path,
            size: state.size.into_inner(),
            bytes_written: state.written.into_inner(),
            missing_segments: missing,
            failed_segments: failed,
        });
    }
    Ok(Report { files: reports })
}

fn unique_name(base: &str, used: &mut HashSet<String>) -> String {
    let mut name = base.to_owned();
    let mut n = 1;
    while !used.insert(name.to_ascii_lowercase()) {
        name = format!("{base}.{n}");
        n += 1;
    }
    name
}

async fn worker(shared: Arc<Shared>) {
    let depth = shared.options.pipeline_depth.max(1);
    loop {
        let batch: Vec<Job> = {
            let mut queue = shared.queue.lock().expect("queue lock");
            let n = depth.min(queue.len());
            queue.drain(..n).collect()
        };
        if batch.is_empty() {
            return;
        }
        let mut conn = match shared.pool.get().await {
            Ok(conn) => conn,
            Err(e) => {
                requeue_or_fail(&shared, &batch, &e.to_string());
                continue;
            }
        };
        let ids: Vec<&str> = batch
            .iter()
            .map(|j| shared.files[j.file].segments[j.segment].message_id.as_str())
            .collect();
        let results = conn.bodies(&ids, depth).await;
        drop(conn);
        for (job, result) in batch.iter().zip(results) {
            match result {
                Ok(body) => store(&shared, *job, &body).await,
                Err(fangst_nntp::Error::NoSuchArticle) => {
                    let number = shared.files[job.file].segments[job.segment].number;
                    shared.files[job.file]
                        .missing
                        .lock()
                        .expect("lock")
                        .push(number);
                    finish_segment(&shared, job.file);
                }
                Err(e) => requeue_or_fail(&shared, std::slice::from_ref(job), &e.to_string()),
            }
        }
    }
}

fn requeue_or_fail(shared: &Shared, jobs: &[Job], reason: &str) {
    for job in jobs {
        if job.attempts < shared.options.retries {
            shared.queue.lock().expect("queue lock").push_back(Job {
                attempts: job.attempts + 1,
                ..*job
            });
        } else {
            let number = shared.files[job.file].segments[job.segment].number;
            shared.files[job.file]
                .failed
                .lock()
                .expect("lock")
                .push((number, reason.to_owned()));
            finish_segment(shared, job.file);
        }
    }
}

async fn store(shared: &Shared, job: Job, body: &[u8]) {
    let state = &shared.files[job.file];
    let number = state.segments[job.segment].number;
    let decoded = match fangst_yenc::decode(body) {
        Ok(decoded) => decoded,
        Err(e) => {
            state
                .failed
                .lock()
                .expect("lock")
                .push((number, e.to_string()));
            finish_segment(shared, job.file);
            return;
        }
    };
    let offset = decoded.part.map_or(0, |p| p.begin - 1);
    let write = async {
        let mut handle = state.handle.lock().await;
        if handle.is_none() {
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&state.path)
                .await?;
            file.set_len(decoded.header.size).await?;
            state.size.store(decoded.header.size, Ordering::Relaxed);
            *handle = Some(file);
        }
        let file = handle.as_mut().expect("opened above");
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        file.write_all(&decoded.data).await
    };
    match write.await {
        Ok(()) => {
            state
                .written
                .fetch_add(decoded.data.len() as u64, Ordering::Relaxed);
        }
        Err(e) => state
            .failed
            .lock()
            .expect("lock")
            .push((number, format!("write failed: {e}"))),
    }
    finish_segment(shared, job.file);
}

fn finish_segment(shared: &Shared, file: usize) {
    let state = &shared.files[file];
    let done = state.done.fetch_add(1, Ordering::Relaxed) + 1;
    (shared.progress)(&Progress {
        file_index: file,
        file_name: state.name.clone(),
        done_segments: done,
        total_segments: state.segments.len(),
        bytes_written: state.written.load(Ordering::Relaxed),
    });
}
