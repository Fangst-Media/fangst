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
    /// Position of the file in the NZB.
    pub index: usize,
    pub name: String,
    pub path: PathBuf,
    /// Size from the yEnc header, 0 if no segment arrived.
    pub size: u64,
    pub bytes_written: u64,
    /// Segments every server answered 430 for.
    pub missing_segments: Vec<u32>,
    /// Segments that arrived damaged or could not be fetched, with the reason.
    pub failed_segments: Vec<(u32, String)>,
    /// Segments stored, including those stored by an earlier run that this one resumed.
    pub segments_done: usize,
    pub segments_total: usize,
}

impl FileReport {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.missing_segments.is_empty()
            && self.failed_segments.is_empty()
            && self.segments_done == self.segments_total
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub files: Vec<FileReport>,
    /// What each server did, in the order the servers were given.
    pub servers: Vec<ServerReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerReport {
    pub name: String,
    pub backfill: bool,
    /// Segments this server delivered.
    pub served: usize,
    /// Segments this server answered 430 for.
    pub missed: usize,
}

/// One usenet server and how it may be used.
#[derive(Clone)]
pub struct Server {
    /// Shown in reports and timelines.
    pub name: String,
    pub pool: Arc<Pool>,
    /// Backfill servers (often block accounts billed per GB) only get segments every primary
    /// server has already reported missing.
    pub backfill: bool,
}

impl Server {
    /// A primary server named after its host.
    #[must_use]
    pub fn primary(pool: Arc<Pool>) -> Self {
        Self {
            name: pool.config().host.clone(),
            pool,
            backfill: false,
        }
    }
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
    /// Downloads need between 1 and 64 servers.
    Servers(usize),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputFolder(msg) => write!(f, "could not use the output folder: {msg}"),
            Self::Servers(n) => write!(f, "a download needs between 1 and 64 servers, got {n}"),
        }
    }
}

impl std::error::Error for Error {}

pub type ProgressFn = Arc<dyn Fn(&Progress) + Send + Sync>;

/// What an earlier run already stored, and where to report segments as they are stored.
#[derive(Clone)]
pub struct Resume {
    /// `(file index in the NZB, segment number)` pairs that are already on disk.
    pub done: HashSet<(usize, u32)>,
    /// Called once a segment's data is written, so the caller can persist it.
    pub on_stored: Arc<dyn Fn(usize, u32) + Send + Sync>,
}

impl Default for Resume {
    fn default() -> Self {
        Self {
            done: HashSet::new(),
            on_stored: Arc::new(|_, _| {}),
        }
    }
}

#[derive(Clone, Copy)]
struct Job {
    file: usize,
    segment: usize,
    attempts: u32,
    /// Bit `i` set: server `i` answered 430 for this segment.
    tried: u64,
}

/// Servers are tracked in a u64 bitmask per segment.
const MAX_SERVERS: usize = 64;

struct FileState {
    /// Position in the NZB; `Job::file` is the position in `Shared::files` instead.
    index: usize,
    name: String,
    path: PathBuf,
    segments: Vec<fangst_nzb::Segment>,
    handle: tokio::sync::Mutex<Option<tokio::fs::File>>,
    size: AtomicU64,
    written: AtomicU64,
    done: AtomicUsize,
    resumed: usize,
    stored: AtomicUsize,
    missing: Mutex<Vec<u32>>,
    failed: Mutex<Vec<(u32, String)>>,
}

struct Shared {
    files: Vec<FileState>,
    queue: Mutex<VecDeque<Job>>,
    servers: Vec<Server>,
    served: Vec<AtomicUsize>,
    missed: Vec<AtomicUsize>,
    /// Jobs taken by a worker and not yet stored, requeued or given up on.
    in_flight: AtomicUsize,
    changed: tokio::sync::Notify,
    options: Options,
    progress: ProgressFn,
    on_stored: Arc<dyn Fn(usize, u32) + Send + Sync>,
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
    download_resumable(nzb, out_dir, pool, options, progress, Resume::default()).await
}

/// Like [`download`], but skips segments `resume` says are done and reports each stored segment.
///
/// # Errors
/// Only when `out_dir` can't be created.
///
/// # Panics
/// If an internal lock was poisoned by a panic in another worker.
pub async fn download_resumable(
    nzb: &Nzb,
    out_dir: &Path,
    pool: Arc<Pool>,
    options: Options,
    progress: ProgressFn,
    resume: Resume,
) -> Result<Report, Error> {
    let name = pool.config().host.clone();
    download_from(
        nzb,
        out_dir,
        vec![Server {
            name,
            pool,
            backfill: false,
        }],
        options,
        progress,
        resume,
    )
    .await
}

/// Downloads from several servers: primaries first, backfill servers only for segments every
/// primary reported missing. A segment counts as missing once every server has reported it missing.
///
/// # Errors
/// When `out_dir` can't be created, or when no servers (or more than 64) are given.
///
/// # Panics
/// If an internal lock was poisoned by a panic in another worker.
pub async fn download_from(
    nzb: &Nzb,
    out_dir: &Path,
    servers: Vec<Server>,
    options: Options,
    progress: ProgressFn,
    resume: Resume,
) -> Result<Report, Error> {
    let all: HashSet<usize> = (0..nzb.files.len()).collect();
    download_selected(nzb, &all, out_dir, servers, options, progress, resume).await
}

/// The name each file of `nzb` is stored under in the output folder, in NZB order.
#[must_use]
pub fn file_names(nzb: &Nzb) -> Vec<String> {
    let mut used_names = HashSet::new();
    nzb.files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let base = file
                .file_name()
                .and_then(|n| fangst_core::path::safe_file_name(&n))
                .unwrap_or_else(|| format!("file{}.bin", i + 1));
            unique_name(&base, &mut used_names)
        })
        .collect()
}

/// Like [`download_from`], but only for the files whose NZB index is in `selected`.
///
/// Files keep the names [`file_names`] gives them, so a later call for other files of the same
/// NZB fills the same folder consistently. The report lists only the selected files.
///
/// # Errors
/// When `out_dir` can't be created, or when no servers (or more than 64) are given.
///
/// # Panics
/// If an internal lock was poisoned by a panic in another worker.
pub async fn download_selected<S: std::hash::BuildHasher>(
    nzb: &Nzb,
    selected: &HashSet<usize, S>,
    out_dir: &Path,
    servers: Vec<Server>,
    options: Options,
    progress: ProgressFn,
    resume: Resume,
) -> Result<Report, Error> {
    if servers.is_empty() || servers.len() > MAX_SERVERS {
        return Err(Error::Servers(servers.len()));
    }
    tokio::fs::create_dir_all(out_dir)
        .await
        .map_err(|e| Error::OutputFolder(e.to_string()))?;

    let files: Vec<FileState> = nzb
        .files
        .iter()
        .zip(file_names(nzb))
        .enumerate()
        .filter(|(i, _)| selected.contains(i))
        .map(|(i, (file, name))| FileState {
            index: i,
            path: out_dir.join(&name),
            name,
            segments: file.segments.clone(),
            handle: tokio::sync::Mutex::new(None),
            size: AtomicU64::new(0),
            written: AtomicU64::new(0),
            done: AtomicUsize::new(
                file.segments
                    .iter()
                    .filter(|seg| resume.done.contains(&(i, seg.number)))
                    .count(),
            ),
            resumed: file
                .segments
                .iter()
                .filter(|seg| resume.done.contains(&(i, seg.number)))
                .count(),
            stored: AtomicUsize::new(0),
            missing: Mutex::new(Vec::new()),
            failed: Mutex::new(Vec::new()),
        })
        .collect();

    let queue = files
        .iter()
        .enumerate()
        .flat_map(|(f, state)| {
            let done = &resume.done;
            state
                .segments
                .iter()
                .enumerate()
                .filter(move |(_, seg)| !done.contains(&(state.index, seg.number)))
                .map(move |(s, _)| Job {
                    file: f,
                    segment: s,
                    attempts: 0,
                    tried: 0,
                })
        })
        .collect();
    let shared = Arc::new(Shared {
        files,
        queue: Mutex::new(queue),
        served: servers.iter().map(|_| AtomicUsize::new(0)).collect(),
        missed: servers.iter().map(|_| AtomicUsize::new(0)).collect(),
        in_flight: AtomicUsize::new(0),
        changed: tokio::sync::Notify::new(),
        servers,
        options,
        progress,
        on_stored: resume.on_stored,
    });

    let mut workers = tokio::task::JoinSet::new();
    for (index, server) in shared.servers.iter().enumerate() {
        let count = options
            .connections
            .max(1)
            .min(server.pool.config().max_connections.max(1));
        for _ in 0..count {
            workers.spawn(worker(Arc::clone(&shared), index));
        }
    }
    while workers.join_next().await.is_some() {}

    let shared = Arc::into_inner(shared).expect("all workers finished");
    Ok(into_report(shared).await)
}

async fn into_report(shared: Shared) -> Report {
    let server_reports = shared
        .servers
        .iter()
        .zip(shared.served)
        .zip(shared.missed)
        .map(|((server, served), missed)| ServerReport {
            name: server.name.clone(),
            backfill: server.backfill,
            served: served.into_inner(),
            missed: missed.into_inner(),
        })
        .collect();
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
            index: state.index,
            name: state.name,
            path: state.path,
            size: state.size.into_inner(),
            bytes_written: state.written.into_inner(),
            missing_segments: missing,
            failed_segments: failed,
            segments_done: state.resumed + state.stored.into_inner(),
            segments_total: state.segments.len(),
        });
    }
    Report {
        files: reports,
        servers: server_reports,
    }
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

fn eligible(shared: &Shared, job: &Job, server: usize) -> bool {
    if job.tried & (1 << server) != 0 {
        return false;
    }
    if !shared.servers[server].backfill {
        return true;
    }
    shared
        .servers
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.backfill)
        .all(|(i, _)| job.tried & (1 << i) != 0)
}

fn take_batch(shared: &Shared, server: usize, depth: usize) -> Vec<Job> {
    let mut queue = shared.queue.lock().expect("queue lock");
    let mut batch = Vec::new();
    let mut i = 0;
    while i < queue.len() && batch.len() < depth {
        if eligible(shared, &queue[i], server) {
            batch.extend(queue.remove(i));
        } else {
            i += 1;
        }
    }
    shared.in_flight.fetch_add(batch.len(), Ordering::SeqCst);
    batch
}

async fn worker(shared: Arc<Shared>, server: usize) {
    let depth = shared.options.pipeline_depth.max(1);
    loop {
        let batch = take_batch(&shared, server, depth);
        if batch.is_empty() {
            let done = shared.queue.lock().expect("queue lock").is_empty()
                && shared.in_flight.load(Ordering::SeqCst) == 0;
            if done {
                shared.changed.notify_waiters();
                return;
            }
            // Work may come back for this server when another server reports a segment missing.
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(50),
                shared.changed.notified(),
            )
            .await;
            continue;
        }
        let pool = Arc::clone(&shared.servers[server].pool);
        match pool.get().await {
            Ok(mut conn) => {
                let ids: Vec<&str> = batch
                    .iter()
                    .map(|j| shared.files[j.file].segments[j.segment].message_id.as_str())
                    .collect();
                let results = conn.bodies(&ids, depth).await;
                drop(conn);
                for (job, result) in batch.iter().zip(results) {
                    match result {
                        Ok(body) => {
                            shared.served[server].fetch_add(1, Ordering::Relaxed);
                            store(&shared, *job, &body).await;
                        }
                        Err(fangst_nntp::Error::NoSuchArticle) => {
                            shared.missed[server].fetch_add(1, Ordering::Relaxed);
                            missing_on(&shared, *job, server);
                        }
                        Err(e) => {
                            requeue_or_fail(&shared, std::slice::from_ref(job), &e.to_string());
                        }
                    }
                }
            }
            Err(e) => requeue_or_fail(&shared, &batch, &e.to_string()),
        }
        // Requeues above happen before this, so nobody sees an empty queue with nothing in flight
        // while a segment is about to come back.
        shared.in_flight.fetch_sub(batch.len(), Ordering::SeqCst);
        shared.changed.notify_waiters();
    }
}

fn missing_on(shared: &Shared, job: Job, server: usize) {
    let tried = job.tried | (1 << server);
    let all = shared.servers.len();
    let everyone_tried = (0..all).all(|i| tried & (1 << i) != 0);
    if everyone_tried {
        let number = shared.files[job.file].segments[job.segment].number;
        shared.files[job.file]
            .missing
            .lock()
            .expect("lock")
            .push(number);
        finish_segment(shared, job.file);
    } else {
        shared
            .queue
            .lock()
            .expect("queue lock")
            .push_back(Job { tried, ..job });
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
            // No truncate: a resumed job keeps the parts an earlier run already wrote.
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&state.path)
                .await?;
            if file.metadata().await?.len() != decoded.header.size {
                file.set_len(decoded.header.size).await?;
            }
            state.size.store(decoded.header.size, Ordering::Relaxed);
            *handle = Some(file);
        }
        let file = handle.as_mut().expect("opened above");
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        file.write_all(&decoded.data).await?;
        // The caller records the segment as done next, so it must already be on disk.
        file.flush().await
    };
    match write.await {
        Ok(()) => {
            state
                .written
                .fetch_add(decoded.data.len() as u64, Ordering::Relaxed);
            state.stored.fetch_add(1, Ordering::Relaxed);
            (shared.on_stored)(state.index, number);
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
        file_index: state.index,
        file_name: state.name.clone(),
        done_segments: done,
        total_segments: state.segments.len(),
        bytes_written: state.written.load(Ordering::Relaxed),
    });
}
