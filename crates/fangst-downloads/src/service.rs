//! The long-running side of the queue: runs jobs as they arrive and lets callers change them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{Notify, broadcast};
use tokio::task::AbortHandle;

use crate::{Error, Job, Runner, Status, Store};

/// Sent whenever a job is added, changes status or is deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Updated(i64),
    Deleted(i64),
}

#[derive(Clone)]
pub struct Service {
    runner: Runner,
    wake: Arc<Notify>,
    running: Arc<Mutex<HashMap<i64, AbortHandle>>>,
    changes: broadcast::Sender<Change>,
}

impl Service {
    #[must_use]
    pub fn new(runner: Runner) -> Self {
        // Slow subscribers miss old changes rather than blocking the queue; they re-read state.
        let (changes, _) = broadcast::channel(256);
        Self {
            runner,
            wake: Arc::new(Notify::new()),
            running: Arc::new(Mutex::new(HashMap::new())),
            changes,
        }
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

    /// Stops a queued or running job until it is resumed. Finished segments are kept.
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
        self.wake.notify_one();
        self.runner.store.get(id).await
    }

    /// Removes a job, stopping it first if it is running. Files on disk are only removed when asked.
    ///
    /// # Errors
    /// [`Error::NotFound`] for an unknown id, or when the files can't be removed.
    pub async fn delete(&self, id: i64, delete_files: bool) -> Result<(), Error> {
        let job = self.runner.store.get(id).await?;
        self.cancel(id);
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

    /// Runs queued jobs one at a time until the task is dropped.
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
        loop {
            let Some(job) = self.runner.store.next_queued().await? else {
                // A fallback poll covers jobs queued by another process sharing the database.
                let _ = tokio::time::timeout(Duration::from_secs(30), self.wake.notified()).await;
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
        }
    }
}
