//! Live byte counts of running jobs, and the rate and time left measured from them.
//!
//! Engines only bump a counter; the service samples every meter on a timer, which keeps the
//! per-segment path free of locks and database writes and caps how often progress goes out.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{Error, Store};

/// How far a running job has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    pub downloaded_bytes: u64,
    /// Averaged over the last few seconds; 0 until measured.
    pub rate_bytes_per_sec: u64,
    /// `None` while the rate or the size is unknown.
    pub eta_seconds: Option<u64>,
}

type Source = Box<dyn Fn() -> u64 + Send + Sync>;

/// One running job's byte count.
#[derive(Default)]
pub(crate) struct Meter {
    downloaded: AtomicU64,
    /// Bytes this run expects to have on disk when it is done; 0 while unknown.
    target: AtomicU64,
    /// Read instead of `downloaded` when an engine keeps its own count.
    source: Mutex<Option<Source>>,
    samples: Mutex<VecDeque<(Instant, u64)>>,
    rate: AtomicU64,
}

impl Meter {
    pub(crate) fn add(&self, bytes: u64) {
        self.downloaded.fetch_add(bytes, Ordering::Relaxed);
    }

    pub(crate) fn add_target(&self, bytes: u64) {
        self.target.fetch_add(bytes, Ordering::Relaxed);
    }

    pub(crate) fn set_target(&self, bytes: u64) {
        self.target.store(bytes, Ordering::Relaxed);
    }

    /// Reads the count from `source` from now on.
    pub(crate) fn follow(&self, source: impl Fn() -> u64 + Send + Sync + 'static) {
        *self.source.lock().expect("meter lock") = Some(Box::new(source));
    }

    /// Stops following a source and keeps `bytes` as the count.
    pub(crate) fn settle(&self, bytes: u64) {
        *self.source.lock().expect("meter lock") = None;
        self.downloaded.store(bytes, Ordering::Relaxed);
    }

    pub(crate) fn downloaded(&self) -> u64 {
        match self.source.lock().expect("meter lock").as_ref() {
            Some(source) => source(),
            None => self.downloaded.load(Ordering::Relaxed),
        }
    }

    /// The progress as of the last sample.
    pub(crate) fn snapshot(&self) -> Progress {
        self.progress(self.downloaded(), self.rate.load(Ordering::Relaxed))
    }

    /// Records the count at `now` and measures the rate over the samples within `window`.
    pub(crate) fn sample(&self, now: Instant, window: Duration) -> Progress {
        let downloaded = self.downloaded();
        let rate = {
            let mut samples = self.samples.lock().expect("meter lock");
            samples.push_back((now, downloaded));
            // The oldest kept sample is the baseline, so one sample at or beyond the window stays.
            while samples.len() > 2
                && samples
                    .get(1)
                    .is_some_and(|(at, _)| now.duration_since(*at) >= window)
            {
                samples.pop_front();
            }
            rate_of(&samples)
        };
        self.rate.store(rate, Ordering::Relaxed);
        self.progress(downloaded, rate)
    }

    fn progress(&self, downloaded: u64, rate: u64) -> Progress {
        let target = self.target.load(Ordering::Relaxed);
        let eta_seconds =
            (rate > 0 && target > 0).then(|| target.saturating_sub(downloaded).div_ceil(rate));
        Progress {
            downloaded_bytes: downloaded,
            rate_bytes_per_sec: rate,
            eta_seconds,
        }
    }
}

fn rate_of(samples: &VecDeque<(Instant, u64)>) -> u64 {
    let (Some((first_at, first)), Some((last_at, last))) = (samples.front(), samples.back()) else {
        return 0;
    };
    let elapsed = last_at.duration_since(*first_at).as_millis();
    if elapsed == 0 {
        return 0;
    }
    let bytes = u128::from(last.saturating_sub(*first));
    u64::try_from(bytes * 1000 / elapsed).unwrap_or(u64::MAX)
}

/// Meters of the jobs running in this process, by job id.
#[derive(Clone, Default)]
pub(crate) struct Meters(Arc<Mutex<HashMap<i64, Arc<Meter>>>>);

impl Meters {
    pub(crate) fn get(&self, id: i64) -> Option<Arc<Meter>> {
        self.0.lock().expect("meters lock").get(&id).cloned()
    }

    pub(crate) fn all(&self) -> Vec<(i64, Arc<Meter>)> {
        self.0
            .lock()
            .expect("meters lock")
            .iter()
            .map(|(id, meter)| (*id, Arc::clone(meter)))
            .collect()
    }

    fn insert(&self, id: i64) -> Arc<Meter> {
        let meter = Arc::new(Meter::default());
        self.0
            .lock()
            .expect("meters lock")
            .insert(id, Arc::clone(&meter));
        meter
    }

    /// Removes `meter` unless a newer run of the same job has replaced it.
    fn remove(&self, id: i64, meter: &Arc<Meter>) {
        let mut meters = self.0.lock().expect("meters lock");
        if meters.get(&id).is_some_and(|m| Arc::ptr_eq(m, meter)) {
            meters.remove(&id);
        }
    }
}

/// A job's meter for as long as it runs. Dropping it (a paused or deleted job's task is aborted)
/// still writes the count, in the background.
pub(crate) struct Running {
    store: Store,
    id: i64,
    pub(crate) meter: Arc<Meter>,
    finished: bool,
}

impl Store {
    pub(crate) fn start_meter(&self, id: i64) -> Running {
        Running {
            store: self.clone(),
            id,
            meter: self.meters.insert(id),
            finished: false,
        }
    }

    /// Records a running job's downloaded bytes. Jobs no longer downloading keep what they have,
    /// so a late write can't undo the count a completed job was given.
    ///
    /// # Errors
    /// When the database write fails.
    pub(crate) async fn set_downloaded(&self, id: i64, bytes: u64) -> Result<(), Error> {
        sqlx::query("UPDATE jobs SET downloaded_bytes = ? WHERE id = ? AND status = 'downloading'")
            .bind(i64::try_from(bytes).unwrap_or(i64::MAX))
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The live progress of a running job, if this process runs it.
    #[must_use]
    pub fn progress(&self, id: i64) -> Option<Progress> {
        self.meters.get(id).map(|meter| meter.snapshot())
    }
}

impl Running {
    /// Writes the final count and stops tracking the job.
    pub(crate) async fn finish(mut self) -> Result<(), Error> {
        self.finished = true;
        let written = self
            .store
            .set_downloaded(self.id, self.meter.downloaded())
            .await;
        self.store.meters.remove(self.id, &self.meter);
        written
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.store.meters.remove(self.id, &self.meter);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let store = self.store.clone();
            let (id, bytes) = (self.id, self.meter.downloaded());
            runtime.spawn(async move {
                let _ = store.set_downloaded(id, bytes).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(5);

    #[test]
    fn rate_is_bytes_over_the_window() {
        let meter = Meter::default();
        meter.set_target(10_000);
        let start = Instant::now();
        let first = meter.sample(start, WINDOW);
        assert_eq!(first.rate_bytes_per_sec, 0);
        assert_eq!(first.eta_seconds, None, "no rate yet");

        meter.add(1000);
        let second = meter.sample(start + Duration::from_secs(1), WINDOW);
        assert_eq!(second.downloaded_bytes, 1000);
        assert_eq!(second.rate_bytes_per_sec, 1000);
        assert_eq!(second.eta_seconds, Some(9));
    }

    #[test]
    fn old_samples_leave_the_window() {
        let meter = Meter::default();
        let start = Instant::now();
        // Fast at first, then slow: the rate follows once the fast part is out of the window.
        meter.sample(start, WINDOW);
        meter.add(100_000);
        meter.sample(start + Duration::from_secs(1), WINDOW);
        for second in 2..=12 {
            meter.add(100);
            meter.sample(start + Duration::from_secs(second), WINDOW);
        }
        assert_eq!(meter.snapshot().rate_bytes_per_sec, 100);
    }

    #[test]
    fn eta_rounds_up_and_needs_a_size() {
        let meter = Meter::default();
        let start = Instant::now();
        meter.sample(start, WINDOW);
        meter.add(300);
        let unknown = meter.sample(start + Duration::from_secs(1), WINDOW);
        assert_eq!(unknown.eta_seconds, None, "size unknown");
        meter.set_target(1000);
        assert_eq!(meter.snapshot().eta_seconds, Some(3), "700 bytes at 300/s");
    }

    #[test]
    fn a_source_replaces_the_counter() {
        let meter = Meter::default();
        meter.add(5);
        meter.follow(|| 42);
        assert_eq!(meter.downloaded(), 42);
    }
}
