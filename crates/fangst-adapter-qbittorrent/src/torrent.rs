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
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
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

    /// qBittorrent 5 state names. A finished job has left the engine and seeds no more, so it
    /// is `stoppedUP`; a job still waiting for a magnet link's metadata is `metaDL`.
    fn state(&self) -> &'static str {
        match self.job.status.as_str() {
            "queued" => "queuedDL",
            "downloading" if self.status.files.is_empty() => "metaDL",
            "downloading" => "downloading",
            "paused" => "stoppedDL",
            "completed" => "stoppedUP",
            _ => "error",
        }
    }

    /// Seeding stops when the download finishes, so for a finished torrent the share limit has
    /// been reached: a limit of 0 against a ratio of 0. Sonarr and Radarr remove a finished
    /// torrent after import only once its limit is reached; without this they never would.
    fn share_limit(&self) -> (f64, i64) {
        if self.done() { (0.0, 0) } else { (-2.0, -2) }
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
            "seeding_time": 0,
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
            "seeding_time": 0,
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
