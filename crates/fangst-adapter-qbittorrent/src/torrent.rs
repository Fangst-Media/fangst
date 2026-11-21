//! A torrent job as qBittorrent describes a torrent: the `torrents/info` item, `properties` and
//! `files`. Every count is a JSON integer and every ratio a float, since Go clients refuse a
//! whole list over one mistyped field.

use std::path::Path;

use fangst_providers::{JobSummary, TorrentStatus};
use serde_json::{Value, json};

/// qBittorrent's "no estimate" (100 days). Sonarr and `HolyArrs` both treat it as unknown.
pub(crate) const MAX_ETA: u64 = 8_640_000;

/// One torrent job with what the queue knows about it.
pub(crate) struct Torrent {
    pub job: JobSummary,
    pub status: TorrentStatus,
}

/// Where the files are, in qBittorrent's terms. `save_path` is the folder the content sits in;
/// `content_path` is the single file, or the folder holding a multi-file torrent. Sonarr imports
/// from `content_path` (or `root_path`) and flags the item when it equals `save_path`.
pub(crate) struct Paths {
    pub save_path: String,
    pub content_path: String,
    pub root_path: String,
    /// What `files[].name` is relative to `save_path` starts with; empty for a single file.
    pub prefix: String,
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Torrent {
    fn done(&self) -> bool {
        self.job.status == "completed"
    }

    fn total(&self) -> u64 {
        u64::try_from(self.job.total_bytes).unwrap_or(0)
    }

    /// The queue's progress meter, or the engine's own count when that is further along: the
    /// meter is sampled about once a second.
    pub(crate) fn have(&self) -> u64 {
        if self.done() {
            return self.total();
        }
        u64::try_from(self.job.downloaded_bytes)
            .unwrap_or(0)
            .max(self.status.have_bytes)
            .min(self.total())
    }

    /// The queue's averaged rate; the engine's current one before the first average exists.
    pub(crate) fn download_bps(&self) -> u64 {
        match u64::try_from(self.job.rate_bytes_per_sec) {
            Ok(rate) if rate > 0 => rate,
            _ => self.status.download_bps,
        }
    }

    pub(crate) fn hash(&self) -> &str {
        &self.status.info_hash
    }

    /// Fangst puts each job's files straight into the job folder, so a single-file torrent's
    /// folder is its save path and a multi-file torrent's folder is its content.
    pub(crate) fn paths(&self, root: &Path) -> Paths {
        let folder = self
            .job
            .output_path
            .clone()
            .unwrap_or_else(|| root.join(&self.job.name));
        if let [only] = self.status.files.as_slice() {
            let file = folder.join(&only.path);
            return Paths {
                save_path: text(&folder),
                content_path: text(&file),
                root_path: String::new(),
                prefix: String::new(),
            };
        }
        let parent = folder
            .parent()
            .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
        let prefix = folder
            .file_name()
            .map_or_default(|n| n.to_string_lossy().into_owned());
        Paths {
            save_path: text(&parent),
            content_path: text(&folder),
            root_path: text(&folder),
            prefix,
        }
    }

    fn progress(&self) -> f64 {
        if self.done() {
            return 1.0;
        }
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)] // A ratio for display; 2^52 bytes is far away.
        let ratio = self.have() as f64 / total as f64;
        ratio
    }

    fn ratio(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)] // As above.
        let ratio = self.status.uploaded_bytes as f64 / total as f64;
        ratio
    }

    fn eta(&self) -> u64 {
        if self.job.status != "downloading" {
            return MAX_ETA;
        }
        if let Some(seconds) = self.job.eta_seconds.and_then(|s| u64::try_from(s).ok()) {
            return seconds.min(MAX_ETA);
        }
        match self.download_bps() {
            0 => MAX_ETA,
            speed => (self.total().saturating_sub(self.have()))
                .div_ceil(speed)
                .min(MAX_ETA),
        }
    }

    /// qBittorrent 5 state names. A finished job is `uploading` while it sends to peers,
    /// `stalledUP` while it seeds with nobody asking, and `stoppedUP` once seeding ended; a job
    /// still waiting for a magnet link's metadata is `metaDL`.
    fn state(&self) -> &'static str {
        match self.job.status.as_str() {
            "queued" => "queuedDL",
            "downloading" if self.status.files.is_empty() => "metaDL",
            "downloading" => "downloading",
            "paused" => "stoppedDL",
            "completed" if self.status.seeding && self.status.upload_bps > 0 => "uploading",
            "completed" if self.status.seeding => "stalledUP",
            "completed" => "stoppedUP",
            _ => "error",
        }
    }

    /// `ratio_limit` and `seeding_time_limit` (minutes), where -1 is no limit and -2 the global
    /// one. Sonarr and Radarr remove a finished torrent after import only when it is `stoppedUP`
    /// and `ratio` or `seeding_time` has reached one of these, so a torrent that stopped seeding
    /// reports the limit it reached. One that stopped for another reason (an engine error, or it
    /// finished before seeding existed) reports 0, which every ratio has reached.
    fn share_limit(&self) -> (f64, i64) {
        if !self.done() {
            return (-2.0, -2);
        }
        let ratio_limit = self.status.ratio_limit.unwrap_or(-1.0);
        let time_limit_secs = self.status.seeding_time_limit_secs;
        let minutes = |secs: u64| i64::try_from(secs / 60).unwrap_or(i64::MAX);
        let time_limit = time_limit_secs.map_or(-1, minutes);
        if self.status.seeding {
            return (ratio_limit, time_limit);
        }
        let ratio_reached = self.status.ratio_limit.is_some_and(|limit| {
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            let needed = (limit.max(0.0) * self.total() as f64).ceil() as u64;
            self.status.uploaded_bytes >= needed
        });
        if ratio_reached {
            return (ratio_limit, time_limit);
        }
        // Rounded down to whole minutes, so the seconds seeded always reach it.
        match time_limit_secs {
            Some(limit) if self.status.seeding_secs >= limit => (ratio_limit, minutes(limit)),
            _ => (0.0, 0),
        }
    }

    fn completion_on(&self) -> i64 {
        if self.done() {
            self.job.finished_at.unwrap_or(-1)
        } else {
            -1
        }
    }

    pub(crate) fn magnet(&self) -> String {
        let name: String = form_urlencoded::byte_serialize(self.job.name.as_bytes()).collect();
        format!("magnet:?xt=urn:btih:{}&dn={name}", self.hash())
    }

    /// The `torrents/info` item; `sync/maindata` uses the same fields keyed by hash.
    pub(crate) fn info(&self, root: &Path) -> Value {
        let paths = self.paths(root);
        let total = self.total();
        let have = self.have();
        let (ratio_limit, seeding_time_limit) = self.share_limit();
        json!({
            "hash": self.hash(),
            "infohash_v1": self.hash(),
            "infohash_v2": "",
            "name": self.job.name,
            "magnet_uri": self.magnet(),
            "size": total,
            "total_size": total,
            "progress": self.progress(),
            "dlspeed": self.download_bps(),
            "upspeed": self.status.upload_bps,
            "priority": 0,
            "num_seeds": 0,
            "num_complete": 0,
            "num_leechs": 0,
            "num_incomplete": 0,
            "ratio": self.ratio(),
            "popularity": 0.0,
            "eta": self.eta(),
            "state": self.state(),
            "seq_dl": false,
            "f_l_piece_prio": false,
            "category": self.job.category.clone().unwrap_or_default(),
            "tags": "",
            "super_seeding": false,
            "force_start": false,
            "save_path": paths.save_path,
            "download_path": "",
            "content_path": paths.content_path,
            "root_path": paths.root_path,
            "added_on": self.job.added_at,
            "completion_on": self.completion_on(),
            "tracker": "",
            "trackers_count": 0,
            "dl_limit": 0,
            "up_limit": 0,
            "downloaded": have,
            "uploaded": self.status.uploaded_bytes,
            "downloaded_session": have,
            "uploaded_session": self.status.uploaded_bytes,
            "amount_left": total - have,
            "completed": have,
            "max_ratio": -1,
            "max_seeding_time": -1,
            "max_inactive_seeding_time": -1,
            "ratio_limit": ratio_limit,
            "seeding_time_limit": seeding_time_limit,
            "inactive_seeding_time_limit": -2,
            "seen_complete": self.completion_on().max(0),
            "last_activity": self.job.finished_at.unwrap_or(self.job.added_at),
            "auto_tmm": false,
            "time_active": 0,
            "seeding_time": self.status.seeding_secs,
            "availability": -1.0,
            "reannounce": 0,
            "comment": "",
            "private": false,
            "has_metadata": !self.status.files.is_empty(),
            "has_tracker_error": false,
            "has_tracker_warning": false,
            "has_other_announce_error": false,
        })
    }

    pub(crate) fn properties(&self, root: &Path) -> Value {
        let paths = self.paths(root);
        let have = self.have();
        json!({
            "hash": self.hash(),
            "infohash_v1": self.hash(),
            "infohash_v2": "",
            "name": self.job.name,
            "save_path": paths.save_path,
            "download_path": "",
            "comment": "",
            "created_by": "",
            "creation_date": -1,
            "addition_date": self.job.added_at,
            "completion_date": self.completion_on(),
            "last_seen": -1,
            "time_elapsed": 0,
            "seeding_time": self.status.seeding_secs,
            "eta": self.eta(),
            "nb_connections": 0,
            "nb_connections_limit": -1,
            "total_downloaded": have,
            "total_downloaded_session": have,
            "total_uploaded": self.status.uploaded_bytes,
            "total_uploaded_session": self.status.uploaded_bytes,
            "total_wasted": 0,
            "total_size": self.total(),
            "dl_speed": self.download_bps(),
            "dl_speed_avg": 0,
            "up_speed": self.status.upload_bps,
            "up_speed_avg": 0,
            "dl_limit": -1,
            "up_limit": -1,
            "seeds": 0,
            "seeds_total": 0,
            "peers": 0,
            "peers_total": 0,
            "share_ratio": self.ratio(),
            "popularity": 0.0,
            "reannounce": 0,
            "pieces_num": 0,
            "piece_size": 0,
            "pieces_have": 0,
            "is_private": false,
            "private": false,
            "has_metadata": !self.status.files.is_empty(),
        })
    }

    /// Per-file progress isn't tracked, so each file shows the torrent's.
    pub(crate) fn files(&self, root: &Path) -> Value {
        let paths = self.paths(root);
        let progress = self.progress();
        let files: Vec<Value> = self
            .status
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let name = if paths.prefix.is_empty() {
                    file.path.clone()
                } else {
                    format!("{}/{}", paths.prefix, file.path)
                };
                // qBittorrent puts `is_seed` on the first file only; no client reads it, and
                // one shape for every item is simpler for strict decoders.
                json!({
                    "index": index,
                    "name": name,
                    "size": file.bytes,
                    "progress": progress,
                    "priority": 1,
                    "availability": -1.0,
                    "piece_range": [0, 0],
                    "is_seed": self.done(),
                })
            })
            .collect();
        Value::Array(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: u64 = 1_000;

    fn finished(seeding: bool, uploaded: u64, seeded_secs: u64, upload_bps: u64) -> Torrent {
        Torrent {
            job: JobSummary {
                id: 1,
                kind: "torrent".to_owned(),
                name: "x".to_owned(),
                category: None,
                priority: 0,
                status: "completed".to_owned(),
                error: None,
                output_path: None,
                total_bytes: 1_000,
                downloaded_bytes: 1_000,
                rate_bytes_per_sec: 0,
                eta_seconds: None,
                added_at: 0,
                finished_at: Some(1),
            },
            status: TorrentStatus {
                info_hash: "a".repeat(40),
                files: Vec::new(),
                have_bytes: SIZE,
                uploaded_bytes: uploaded,
                download_bps: 0,
                upload_bps,
                seeding,
                seeding_secs: seeded_secs,
                ratio_limit: Some(1.0),
                seeding_time_limit_secs: Some(90),
            },
        }
    }

    /// Sonarr and Radarr remove a finished torrent once it is `stoppedUP` or `pausedUP` and its
    /// ratio has reached `ratio_limit` or `seeding_time` (seconds) has reached
    /// `seeding_time_limit` (minutes).
    fn removable(torrent: &Torrent) -> bool {
        let (ratio_limit, time_limit) = torrent.share_limit();
        let stopped = torrent.state() == "stoppedUP";
        let by_ratio = ratio_limit >= 0.0 && torrent.ratio() >= ratio_limit - 0.001;
        let by_time = time_limit >= 0
            && i64::try_from(torrent.status.seeding_secs).unwrap() >= time_limit * 60;
        stopped && (by_ratio || by_time)
    }

    #[test]
    fn a_seeding_torrent_uploads_and_is_not_removed() {
        let busy = finished(true, 10, 5, 2_048);
        assert_eq!(busy.state(), "uploading");
        assert_eq!(finished(true, 10, 5, 0).state(), "stalledUP");
        let (ratio, minutes) = busy.share_limit();
        assert!((ratio - 1.0).abs() < f64::EPSILON);
        assert_eq!(minutes, 1, "90 seconds, rounded down to whole minutes");
        assert!(!removable(&busy));
    }

    #[test]
    fn a_torrent_that_stopped_seeding_is_removed() {
        let by_ratio = finished(false, SIZE, 5, 0);
        assert_eq!(by_ratio.state(), "stoppedUP");
        assert!(removable(&by_ratio));

        let by_time = finished(false, 10, 90, 0);
        assert_eq!(by_time.share_limit().1, 1);
        assert!(removable(&by_time));

        // Neither limit reached: an engine error ended seeding, or it finished before seeding
        // existed.
        let other = finished(false, 0, 0, 0);
        assert_eq!(other.share_limit(), (0.0, 0));
        assert!(removable(&other));
    }
}
