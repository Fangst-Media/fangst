//! The values in `defaults.toml`, typed.

use std::time::Duration;

use serde::Deserialize;

/// Every section of `defaults.toml`. A section unknown here is ignored so a crate can add its own
/// section before it reads it through this type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Defaults {
    pub notify: Notify,
    pub disk: Disk,
    pub progress: Progress,
    pub torrent: Torrent,
    pub extract: Extract,
    pub safety: Safety,
    pub usenet: Usenet,
    pub http: Http,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notify {
    pub timeout_secs: u64,
    /// The ntfy server for notify-ntfy when its settings name none.
    pub ntfy_server: String,
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
#[allow(clippy::struct_excessive_bools)] // Independent switches, each its own setting.
pub struct Torrent {
    /// Torrent jobs downloading at once; the rest wait in the queue.
    pub max_active: usize,
    /// TCP port peers connect to.
    pub listen_port: u16,
    pub enabled: bool,
    /// 0 means `listen_port`.
    pub announce_port: u16,
    pub dht: bool,
    pub local_discovery: bool,
    pub upnp: bool,
    /// Bytes per second; 0 means no limit.
    pub download_limit: u32,
    pub upload_limit: u32,
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

/// What a usenet server gets for each setting it leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usenet {
    pub port: u16,
    pub tls: bool,
    pub connections: usize,
    pub pipeline: usize,
    pub connect_timeout_secs: u64,
    pub command_timeout_secs: u64,
    pub retries: u32,
}

impl Usenet {
    #[must_use]
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }

    #[must_use]
    pub fn command_timeout(&self) -> Duration {
        Duration::from_secs(self.command_timeout_secs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Http {
    pub listen: String,
    pub secure_cookie: bool,
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
    /// Parses a defaults file. The config file layers on top of the built-in one; see
    /// [`crate::config`].
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
    use super::Defaults;

    /// The built-in file with `edit` applied, as text.
    fn builtin_with(edit: impl FnOnce(&mut toml::Table)) -> String {
        let mut table: toml::Table = toml::from_str(crate::DEFAULTS_TOML).unwrap();
        edit(&mut table);
        toml::to_string(&table).unwrap()
    }

    fn section(table: &mut toml::Table, name: &str) -> toml::Table {
        table.get(name).unwrap().as_table().unwrap().clone()
    }

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
        assert!(defaults.torrent.enabled && defaults.torrent.dht);
        assert_eq!(defaults.torrent.download_limit, 0);
        assert_eq!(defaults.usenet.port, 563);
        assert_eq!(defaults.usenet.connections, 8);
        assert_eq!(defaults.http.listen, "127.0.0.1:7740");
        assert_eq!(defaults.notify.ntfy_server, "https://ntfy.sh");
        assert!(defaults.extract.delete_archives);
        assert_eq!(
            defaults.safety.blocked_extensions,
            [
                "exe", "lnk", "scr", "bat", "cmd", "com", "pif", "vbs", "js", "msi", "ps1"
            ]
        );
    }

    #[test]
    fn missing_value_is_an_error() {
        let text = builtin_with(|t| {
            let mut disk = section(t, "disk");
            disk.remove("check_interval_secs");
            t.insert("disk".into(), disk.into());
        });
        let err = Defaults::parse(&text).unwrap_err();
        assert!(err.to_string().contains("check_interval_secs"), "{err}");
    }

    #[test]
    fn misspelled_key_is_an_error() {
        let text = builtin_with(|t| {
            let mut notify = section(t, "notify");
            let value = notify.remove("timeout_secs").unwrap();
            notify.insert("timeout_sec".into(), value);
            t.insert("notify".into(), notify.into());
        });
        assert!(Defaults::parse(&text).is_err());
    }

    #[test]
    fn unknown_section_is_ignored() {
        let text = builtin_with(|t| {
            let mut later = toml::Table::new();
            later.insert("x".into(), 1.into());
            t.insert("later".into(), later.into());
        });
        assert_eq!(Defaults::parse(&text).unwrap().notify.timeout_secs, 10);
    }
}
