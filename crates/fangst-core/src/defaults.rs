//! The values in `defaults.toml`, typed.

use std::time::Duration;

use serde::Deserialize;

/// Every section of `defaults.toml`. A section unknown here is ignored so a crate can add its own
/// section before it reads it through this type.
// Not Eq, since the seeding ratio is a float.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Defaults {
    pub notify: Notify,
    pub disk: Disk,
    pub progress: Progress,
    pub torrent: Torrent,
    pub seeding: Seeding,
    pub extract: Extract,
    pub safety: Safety,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notify {
    pub timeout_secs: u64,
}

impl Notify {
    #[must_use]
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disk {
    pub min_free_bytes: u64,
    pub check_interval_secs: u64,
}

impl Disk {
    #[must_use]
    pub fn check_interval(&self) -> Duration {
        Duration::from_secs(self.check_interval_secs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub publish_interval_ms: u64,
    pub rate_window_secs: u64,
    pub persist_interval_secs: u64,
}

impl Progress {
    #[must_use]
    pub fn publish_interval(&self) -> Duration {
        Duration::from_millis(self.publish_interval_ms)
    }

    #[must_use]
    pub fn rate_window(&self) -> Duration {
        Duration::from_secs(self.rate_window_secs)
    }

    #[must_use]
    pub fn persist_interval(&self) -> Duration {
        Duration::from_secs(self.persist_interval_secs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Torrent {
    /// Torrent jobs downloading at once; the rest wait in the queue.
    pub max_active: usize,
    /// TCP port peers connect to.
    pub listen_port: u16,
}

/// When a finished torrent stops seeding: whichever limit comes first. Negative turns one off.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seeding {
    /// Uploaded bytes over the torrent's size.
    pub ratio_limit: f64,
    pub time_limit_secs: i64,
}

impl Seeding {
    #[must_use]
    pub fn ratio_limit(&self) -> Option<f64> {
        (self.ratio_limit >= 0.0).then_some(self.ratio_limit)
    }

    #[must_use]
    pub fn time_limit(&self) -> Option<Duration> {
        u64::try_from(self.time_limit_secs)
            .ok()
            .map(Duration::from_secs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extract {
    pub delete_archives: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Safety {
    /// Without the dot; compared ignoring case.
    pub blocked_extensions: Vec<String>,
}

#[derive(Debug)]
pub struct Error(toml::de::Error);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid defaults: {}", self.0)
    }
}

impl std::error::Error for Error {}

impl Defaults {
    /// Parses a defaults file. Config files (#31) will layer on top of the built-in one.
    ///
    /// # Errors
    /// When the text is not valid TOML or a value is missing or of the wrong type.
    pub fn parse(text: &str) -> Result<Self, Error> {
        toml::from_str(text).map_err(Error)
    }

    /// The defaults compiled into the binary.
    ///
    /// # Errors
    /// Only if `defaults.toml` itself is broken, which its tests catch.
    pub fn builtin() -> Result<Self, Error> {
        Self::parse(crate::DEFAULTS_TOML)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Defaults;

    /// Every section after `[disk]`, which the tests below don't vary.
    const REST: &str = "[progress]\npublish_interval_ms = 1\nrate_window_secs = 1\npersist_interval_secs = 1\n\
        [torrent]\nmax_active = 1\nlisten_port = 1\n\
        [seeding]\nratio_limit = 1.0\ntime_limit_secs = -1\n\
        [extract]\ndelete_archives = true\n\
        [safety]\nblocked_extensions = [\"exe\"]\n";

    #[test]
    fn builtin_defaults_parse() {
        let defaults = Defaults::builtin().unwrap();
        assert_eq!(defaults.notify.timeout_secs, 10);
        assert_eq!(defaults.disk.min_free_bytes, 5 * 1024 * 1024 * 1024);
        assert_eq!(defaults.disk.check_interval_secs, 60);
        assert_eq!(defaults.progress.publish_interval_ms, 1000);
        assert_eq!(defaults.progress.rate_window_secs, 5);
        assert_eq!(defaults.progress.persist_interval_secs, 10);
        assert_eq!(defaults.torrent.max_active, 5);
        assert_eq!(defaults.torrent.listen_port, 6881);
        assert!(
            defaults
                .seeding
                .ratio_limit()
                .is_some_and(|r| (r - 1.0).abs() < f64::EPSILON)
        );
        assert_eq!(
            defaults.seeding.time_limit(),
            Some(Duration::from_hours(7 * 24))
        );
        assert!(defaults.extract.delete_archives);
        assert_eq!(
            defaults.safety.blocked_extensions,
            [
                "exe", "lnk", "scr", "bat", "cmd", "com", "pif", "vbs", "js", "msi", "ps1"
            ]
        );
    }

    #[test]
    fn negative_seeding_limits_are_off() {
        let seeding = super::Seeding {
            ratio_limit: -1.0,
            time_limit_secs: -1,
        };
        assert_eq!(seeding.ratio_limit(), None);
        assert_eq!(seeding.time_limit(), None);
    }

    #[test]
    fn missing_value_is_an_error() {
        let text = format!("[notify]\ntimeout_secs = 1\n[disk]\nmin_free_bytes = 1\n{REST}");
        let err = Defaults::parse(&text).unwrap_err();
        assert!(err.to_string().contains("check_interval_secs"), "{err}");
    }

    #[test]
    fn misspelled_key_is_an_error() {
        let text = format!(
            "[notify]\ntimeout_sec = 1\n[disk]\nmin_free_bytes = 1\ncheck_interval_secs = 1\n{REST}"
        );
        assert!(Defaults::parse(&text).is_err());
    }

    #[test]
    fn unknown_section_is_ignored() {
        let text = format!(
            "[later]\nx = 1\n[notify]\ntimeout_secs = 1\n[disk]\nmin_free_bytes = 1\ncheck_interval_secs = 1\n{REST}"
        );
        assert_eq!(Defaults::parse(&text).unwrap().notify.timeout_secs, 1);
    }
}
