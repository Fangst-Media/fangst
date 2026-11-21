//! Tells the enabled notifiers when jobs finish and when the downloads folder runs low on space.
//!
//! Notifications go out in their own tasks so a slow notifier never holds up the queue, and a
//! failing one only leaves a `notify` entry on the job's timeline.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fangst_providers::{NotificationEvent, Notifier};

use crate::{Status, Store};

pub struct Notifications {
    pub notifiers: Vec<Arc<dyn Notifier>>,
    /// How long one notifier may take for one event.
    pub timeout: Duration,
    /// Free bytes in the downloads folder below which a disk-full notification goes out.
    pub min_free_bytes: u64,
    pub disk_check_interval: Duration,
}

impl Notifications {
    pub(crate) fn none() -> Self {
        Self {
            notifiers: Vec::new(),
            timeout: Duration::ZERO,
            min_free_bytes: 0,
            disk_check_interval: Duration::MAX,
        }
    }
}

/// Announces a job that just stopped running, if it completed or failed.
pub(crate) fn job_finished(notifications: &Arc<Notifications>, store: &Store, id: i64) {
    if notifications.notifiers.is_empty() {
        return;
    }
    let notifications = Arc::clone(notifications);
    let store = store.clone();
    tokio::spawn(async move {
        // Paused, deleted or requeued jobs have nothing to announce.
        let Ok(job) = store.get(id).await else {
            return;
        };
        let event = match job.status {
            Status::Completed => NotificationEvent::JobCompleted {
                job_id: id,
                name: job.name,
                category: job.category,
            },
            Status::Failed => NotificationEvent::JobFailed {
                job_id: id,
                name: job.name,
                reason: job.error.unwrap_or_default(),
            },
            _ => return,
        };
        send(&notifications, &store, Some(id), &event);
    });
}

/// Sends `event` to every notifier at once. Failures go on job `job`'s timeline when there is one.
fn send(notifications: &Notifications, store: &Store, job: Option<i64>, event: &NotificationEvent) {
    let timeout = notifications.timeout;
    for notifier in &notifications.notifiers {
        let notifier = Arc::clone(notifier);
        let store = store.clone();
        let event = event.clone();
        tokio::spawn(async move {
            let id = notifier.info().id;
            let name = event.name();
            let problem = match tokio::time::timeout(timeout, notifier.notify(event)).await {
                Ok(Ok(())) => return,
                Ok(Err(e)) => format!("{id} could not send the {name} notification: {e}"),
                Err(_) => format!(
                    "{id} did not send the {name} notification within {} s",
                    timeout.as_secs_f64()
                ),
            };
            match job {
                Some(job) => {
                    // A job deleted in the meantime has no timeline left to write to.
                    let _ = store.add_event(job, "notify", &problem).await;
                }
                None => eprintln!("{problem}"),
            }
        });
    }
}

/// Checks free space in `root` forever, announcing each drop below the threshold once.
/// Follows `notifications` as settings replace them, and waits while there are no notifiers.
pub(crate) async fn watch_disk(
    mut notifications: tokio::sync::watch::Receiver<Arc<Notifications>>,
    store: Store,
    root: PathBuf,
) {
    let mut crossing = Crossing::default();
    loop {
        let current = Arc::clone(&notifications.borrow_and_update());
        if current.notifiers.is_empty() {
            if notifications.changed().await.is_err() {
                return std::future::pending().await;
            }
            continue;
        }
        let notifications_now = current;
        let measured = root.clone();
        let free = tokio::task::spawn_blocking(move || free_space(&measured)).await;
        if let Ok(Some(free)) = free
            && crossing.observe(free, notifications_now.min_free_bytes)
        {
            let event = NotificationEvent::DiskFull {
                path: root.clone(),
                free_bytes: free,
            };
            send(&notifications_now, &store, None, &event);
        }
        tokio::select! {
            () = tokio::time::sleep(notifications_now.disk_check_interval) => {}
            _ = notifications.changed() => {}
        }
    }
}

/// Free bytes on the disk holding `path`. The downloads folder only exists after the first job, so
/// the nearest folder that exists stands in for it.
fn free_space(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.is_dir())?;
    fs4::available_space(existing).ok()
}

/// Remembers whether free space was already below the threshold, so one shortage is one message.
#[derive(Default)]
struct Crossing {
    below: bool,
}

impl Crossing {
    /// Whether this reading crosses below the threshold.
    fn observe(&mut self, free: u64, threshold: u64) -> bool {
        let below = free < threshold;
        let crossed = below && !self.below;
        self.below = below;
        crossed
    }
}

#[cfg(test)]
mod tests {
    use super::Crossing;

    #[test]
    fn announces_each_crossing_once() {
        let mut crossing = Crossing::default();
        let readings = [100, 50, 40, 30, 120, 90, 80, 200];
        let announced: Vec<bool> = readings
            .iter()
            .map(|free| crossing.observe(*free, 60))
            .collect();
        assert_eq!(
            announced,
            [false, true, false, false, false, false, false, false]
        );

        let mut crossing = Crossing::default();
        let announced: Vec<bool> = [10, 70, 10]
            .iter()
            .map(|free| crossing.observe(*free, 60))
            .collect();
        assert_eq!(announced, [true, false, true]);
    }

    #[test]
    fn exactly_the_threshold_is_not_below_it() {
        assert!(!Crossing::default().observe(60, 60));
    }
}
