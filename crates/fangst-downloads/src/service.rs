//! The long-running side of the queue: runs jobs as they arrive and lets callers change them.
//!
//! Each protocol has its own lane. Usenet jobs run one at a time in priority order, since one job
//! already uses every connection. Torrent jobs all start as soon as they are queued, since each
//! one waits on its own peers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{Notify, broadcast};
use tokio::task::AbortHandle;

use crate::notify::{self, Notifications};
use crate::torrent::{TorrentRunner, TorrentSource};
use crate::{Error, Job, Kind, Runner, Status, Store};

/// A fallback poll covers jobs queued by another process sharing the database.
const IDLE_POLL: Duration = Duration::from_secs(30);

/// Sent whenever a job is added, changes status or is deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Updated(i64),
    Deleted(i64),
}

#[derive(Clone)]
pub struct Service {
    runner: Runner,
    torrents: Option<TorrentRunner>,
    usenet: bool,
    wake: Arc<Notify>,
    wake_torrents: Arc<Notify>,
    running: Arc<Mutex<HashMap<i64, AbortHandle>>>,
    changes: broadcast::Sender<Change>,
    notifications: Arc<Notifications>,
}

impl Service {
    #[must_use]
    pub fn new(runner: Runner) -> Self {
        // Slow subscribers miss old changes rather than blocking the queue; they re-read state.
        let (changes, _) = broadcast::channel(256);
        Self {
            runner,
            torrents: None,
            usenet: true,
            wake: Arc::new(Notify::new()),
            wake_torrents: Arc::new(Notify::new()),
            running: Arc::new(Mutex::new(HashMap::new())),
            changes,
            notifications: Arc::new(Notifications::none()),
        }
    }

    /// Runs torrent jobs on `engine`, in job folders under the usenet runner's root. Without an
    /// engine, torrent jobs can still be added and wait in the queue.
    #[must_use]
    pub fn with_torrents(mut self, engine: fangst_torrent::Engine) -> Self {
        self.torrents = Some(TorrentRunner {
            store: self.runner.store.clone(),
            engine,
            root: self.runner.root.clone(),
        });
        self
    }

    /// Tells `notifications.notifiers` when jobs complete or fail and when disk space runs low.
    #[must_use]
    pub fn with_notifications(mut self, notifications: Notifications) -> Self {
        self.notifications = Arc::new(notifications);
        self
    }

    /// Leaves usenet jobs waiting in the queue, for when no usenet server is configured.
    #[must_use]
    pub fn without_usenet(mut self) -> Self {
        self.usenet = false;
        self
    }

    #[must_use]
    pub fn store(&self) -> &Store {
        &self.runner.store
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.changes.subscribe()
    }

    fn changed(&self, id: i64) {
        let _ = self.changes.send(Change::Updated(id));
    }

    fn wake_all(&self) {
        self.wake.notify_one();
        self.wake_torrents.notify_one();
    }

    /// Queues an NZB and wakes the queue.
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
        let id = self
            .runner
            .store
            .add_nzb(name, category, priority, nzb)
            .await?;
        self.changed(id);
        self.wake.notify_one();
        Ok(id)
    }

    /// Queues a magnet link or `.torrent` file and wakes the queue.
    ///
    /// # Errors
    /// See [`Store::add_torrent`].
    pub async fn add_torrent(
        &self,
        name: &str,
        category: Option<&str>,
        priority: i64,
        source: TorrentSource<'_>,
    ) -> Result<i64, Error> {
        let id = self
            .runner
            .store
            .add_torrent(name, category, priority, source)
            .await?;
        self.changed(id);
        self.wake_torrents.notify_one();
        Ok(id)
    }

    /// Stops a queued or running job until it is resumed. Finished segments and pieces are kept.
    ///
    /// # Errors
    /// [`Error::NotFound`] for an unknown id; completed and failed jobs can't be paused.
    pub async fn pause(&self, id: i64) -> Result<Job, Error> {
        let job = self.runner.store.get(id).await?;
        if !matches!(job.status, Status::Queued | Status::Downloading) {
            return Err(Error::InvalidState {
                id,
                status: job.status,
                action: "pause",
            });
        }
        self.runner
            .store
            .set_status(id, Status::Paused, None)
            .await?;
        self.cancel(id);
        self.changed(id);
        self.runner.store.get(id).await
    }

    /// Puts a paused or failed job back in the queue.
    ///
    /// # Errors
    /// [`Error::NotFound`] for an unknown id; only paused and failed jobs can be resumed.
    pub async fn resume(&self, id: i64) -> Result<Job, Error> {
        let job = self.runner.store.get(id).await?;
        if !matches!(job.status, Status::Paused | Status::Failed) {
            return Err(Error::InvalidState {
                id,
                status: job.status,
                action: "resume",
            });
        }
        self.runner
            .store
            .set_status(id, Status::Queued, None)
            .await?;
        self.changed(id);
        self.wake_all();
        self.runner.store.get(id).await
    }

    /// Removes a job, stopping it first if it is running. Files on disk are only removed when asked.
    ///
    /// # Errors
    /// [`Error::NotFound`] for an unknown id, or when the files can't be removed.
    pub async fn delete(&self, id: i64, delete_files: bool) -> Result<(), Error> {
        let job = self.runner.store.get(id).await?;
        self.cancel(id);
        if job.kind == Kind::Torrent
            && let Some(torrents) = &self.torrents
        {
            // The engine must close the files before they can be removed, on Windows especially.
            let hash = self.runner.store.torrent_info_hash(id).await?;
            torrents.engine.forget(&hash).await;
        }
        self.runner.store.delete(id).await?;
        if delete_files && let Some(path) = job.output_path {
            match tokio::fs::remove_dir_all(&path).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(Error::Download(format!(
                        "could not remove {}: {e}",
                        path.display()
                    )));
                }
            }
        }
        let _ = self.changes.send(Change::Deleted(id));
        Ok(())
    }

    fn cancel(&self, id: i64) {
        if let Some(handle) = self.running.lock().expect("running lock").remove(&id) {
            handle.abort();
        }
    }

    /// Runs queued jobs until the task is dropped.
    ///
    /// # Errors
    /// When the database fails while picking or finishing a job.
    ///
    /// # Panics
    /// If the running-jobs lock was poisoned by a panic elsewhere.
    pub async fn run(&self) -> Result<(), Error> {
        let interrupted = self.runner.store.requeue_interrupted().await?;
        if interrupted > 0 {
            for job in self.runner.store.list().await? {
                self.changed(job.id);
            }
        }
        let disk = async {
            notify::watch_disk(
                Arc::clone(&self.notifications),
                self.runner.store.clone(),
                self.runner.root.clone(),
            )
            .await;
            Ok(())
        };
        tokio::try_join!(self.run_usenet(), self.run_torrents(), disk).map(|_| ())
    }

    async fn run_usenet(&self) -> Result<(), Error> {
        if !self.usenet {
            return std::future::pending().await;
        }
        loop {
            let Some(job) = self.runner.store.next_queued_of(Kind::Usenet).await? else {
                let _ = tokio::time::timeout(IDLE_POLL, self.wake.notified()).await;
                continue;
            };
            let runner = self.runner.clone();
            let changes = self.changes.clone();
            let id = job.id;
            let task = tokio::spawn(async move {
                let _ = changes.send(Change::Updated(id));
                runner.run(&job).await
            });
            self.running
                .lock()
                .expect("running lock")
                .insert(id, task.abort_handle());
            let outcome = task.await;
            self.running.lock().expect("running lock").remove(&id);
            match outcome {
                Ok(result) => result?,
                // Paused or deleted while running; the caller already set the new state.
                Err(e) if e.is_cancelled() => {}
                Err(e) => return Err(Error::Download(e.to_string())),
            }
            self.changed(id);
            notify::job_finished(&self.notifications, &self.runner.store, id);
        }
    }

    async fn run_torrents(&self) -> Result<(), Error> {
        let Some(torrents) = &self.torrents else {
            return std::future::pending().await;
        };
        loop {
            let Some(job) = self.runner.store.next_queued_of(Kind::Torrent).await? else {
                let _ = tokio::time::timeout(IDLE_POLL, self.wake_torrents.notified()).await;
                continue;
            };
            let id = job.id;
            // Set before spawning so the next pick doesn't see this job as queued again.
            self.runner
                .store
                .set_status(id, Status::Downloading, None)
                .await?;
            self.changed(id);
            let runner = torrents.clone();
            let task = tokio::spawn(async move { runner.execute(&job).await });
            self.running
                .lock()
                .expect("running lock")
                .insert(id, task.abort_handle());
            let service = self.clone();
            tokio::spawn(async move {
                let task_id = task.id();
                let outcome = task.await;
                {
                    // A job paused and resumed quickly already has a newer task under its id.
                    let mut running = service.running.lock().expect("running lock");
                    if running.get(&id).is_some_and(|h| h.id() == task_id) {
                        running.remove(&id);
                    }
                }
                if let Ok(Err(e)) = outcome {
                    // The database failed mid-job; record it if it has recovered.
                    let _ = service
                        .runner
                        .store
                        .set_status(id, Status::Failed, Some(&e.to_string()))
                        .await;
                }
                service.changed(id);
                notify::job_finished(&service.notifications, &service.runner.store, id);
            });
        }
    }
}
