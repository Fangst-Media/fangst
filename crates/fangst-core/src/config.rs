//! The config file, `<data-dir>/fangst.toml`, and how it layers over the built-in defaults.
//!
//! Four layers, each overriding the one before: the built-in `defaults.toml`, the config file,
//! `FANGST_<SECTION>__<KEY>` environment variables, and command-line flags. Every layer is a TOML
//! table and they merge key by key, so a layer only holds the keys it changes. The file holds the
//! tunables from `defaults.toml` plus the sections in [`Settings`]. Secrets never live in it: a
//! setting names an environment variable instead, or the daemon keeps the value encrypted.

use std::collections::BTreeSet;
use std::fmt;
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
pub use toml::{Table, Value};

use crate::defaults::Defaults;

pub const FILE_NAME: &str = "fangst.toml";

/// Environment variables starting with this, followed by `<SECTION>__<KEY>`, override a setting.
pub const ENV_PREFIX: &str = "FANGST_";
const ENV_SEPARATOR: &str = "__";

/// Sections of the file that are settings rather than tunables from `defaults.toml`.
pub const SETTINGS_SECTIONS: [&str; 6] = [
    "folders",
    "servers",
    "categories",
    "notifiers",
    "providers",
    "setup",
];

/// Folder a relative `folders.downloads` and a missing one resolve to, inside the data folder.
pub const DEFAULT_DOWNLOADS: &str = "downloads";

/// Upper bound on connections to one server. Providers allow at most 100 or so; more only gets
/// refused, and every connection holds a socket and a buffer.
pub const MAX_CONNECTIONS: usize = 200;

/// Category names end up in folder names and other products' APIs; one line of printable text.
const MAX_NAME_CHARS: usize = 255;

const HEADER: &str = "\
# Fangst settings. The web UI rewrites this file when settings change; comments are not kept.
# Any value from defaults.toml (see TUNING.md) can be overridden here under its own section.
# Passwords and tokens are never stored here: set them in the web UI, which keeps them
# encrypted in downloads.db, or name an environment variable with `password_env`/`token_env`.
";

/// Written on first start, so the file exists to edit.
const TEMPLATE: &str = "
[folders]
# Where downloads go; relative to the data folder.
downloads = \"downloads\"

[providers]
# Optional parts to switch on, such as \"adapter-sabnzbd\" or \"notify-ntfy\".
enabled = []
";

#[derive(Debug)]
pub enum Error {
    Io(PathBuf, io::Error),
    /// The file or a variable is not valid TOML, or a value has the wrong type.
    Parse(String),
    /// Valid TOML that breaks a rule, such as two servers with the same name.
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
            Self::Parse(why) | Self::Invalid(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for Error {}

/// Everything the settings sections of the file hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub folders: Folders,
    pub servers: Vec<Server>,
    pub categories: Vec<Category>,
    pub notifiers: Notifiers,
    pub providers: Providers,
    pub setup: Setup,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Folders {
    /// Relative paths are inside the data folder. Defaults to [`DEFAULT_DOWNLOADS`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads: Option<String>,
    /// The media library, checked for hardlinks with the downloads folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
}

impl Folders {
    #[must_use]
    pub fn downloads_path(&self, data_dir: &Path) -> PathBuf {
        resolve(
            data_dir,
            self.downloads.as_deref().unwrap_or(DEFAULT_DOWNLOADS),
        )
    }

    #[must_use]
    pub fn media_path(&self, data_dir: &Path) -> Option<PathBuf> {
        self.media.as_deref().map(|m| resolve(data_dir, m))
    }
}

/// `path` as is when absolute, else inside `base`.
#[must_use]
pub fn resolve(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    }
}

/// One usenet server. Settings left out take the `[usenet]` values from the defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    /// Stable name for the server's stored password; filled from `name` when left out.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Environment variable holding the password, instead of the encrypted store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connections: Option<usize>,
    /// Lower goes first; servers with the same priority keep their order in the file.
    #[serde(default)]
    pub priority: i64,
    /// Only gets segments every primary server reported missing.
    #[serde(default)]
    pub backfill: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Never written. Read only to refuse it with a message saying where passwords go.
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub name: String,
    /// Jobs in the category download into this folder; relative paths are inside the downloads
    /// folder. Without it they go straight into the downloads folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// Where each notifier sends to. Whether one runs is up to `providers.enabled`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Notifiers {
    pub webhook: Webhook,
    pub ntfy: Ntfy,
    pub discord: Discord,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Webhook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Environment variable holding the bearer token, instead of the encrypted store.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ntfy {
    /// Defaults to `notify.ntfy_server`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Discord {
    /// Environment variable holding the webhook URL, which carries its own token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url_env: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Providers {
    /// Provider ids to switch on. In an environment variable, a comma-separated list works too.
    #[serde(deserialize_with = "list_or_commas")]
    pub enabled: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Setup {
    /// Set when the first-run setup in the web UI finished, or skipped by environment variables.
    pub completed: bool,
}

fn list_or_commas<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ListOrText {
        List(Vec<String>),
        Text(String),
    }
    Ok(match ListOrText::deserialize(deserializer)? {
        ListOrText::List(list) => list,
        ListOrText::Text(text) => text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
    })
}

/// The effective values once every layer is merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub defaults: Defaults,
    pub settings: Settings,
}

/// The four layers, kept apart so the file can be rewritten without the others leaking into it.
#[derive(Debug, Clone)]
pub struct Layers {
    pub defaults: Table,
    pub file: Table,
    pub env: Table,
    pub flags: Table,
}

impl Layers {
    /// The built-in defaults under `file`, `env` and `flags`.
    ///
    /// # Errors
    /// Only if the built-in defaults file is broken, which its tests catch.
    pub fn new(file: Table, env: Table, flags: Table) -> Result<Self, Error> {
        Ok(Self {
            defaults: toml::from_str(crate::DEFAULTS_TOML)
                .map_err(|e| Error::Parse(format!("defaults.toml: {e}")))?,
            file,
            env,
            flags,
        })
    }

    #[must_use]
    pub fn with_file(&self, file: Table) -> Self {
        Self {
            file,
            ..self.clone()
        }
    }

    #[must_use]
    pub fn merged(&self) -> Table {
        let mut merged = self.defaults.clone();
        for layer in [&self.file, &self.env, &self.flags] {
            merge(&mut merged, layer.clone());
        }
        merged
    }

    /// Merges and checks every layer.
    ///
    /// # Errors
    /// [`Error::Parse`] for an unknown section or key or a value of the wrong type,
    /// [`Error::Invalid`] for settings that break a rule.
    pub fn resolve(&self) -> Result<Config, Error> {
        self.check_sections(&self.file, FILE_NAME)?;
        self.check_sections(&self.env, "the environment")?;
        let merged = Value::Table(self.merged());
        let defaults: Defaults = merged
            .clone()
            .try_into()
            .map_err(|e| Error::Parse(format!("settings: {e}")))?;
        let mut settings: Settings = merged
            .try_into()
            .map_err(|e| Error::Parse(format!("settings: {e}")))?;
        for server in &mut settings.servers {
            if server.id.trim().is_empty() {
                server.id.clone_from(&server.name);
            }
        }
        settings.validate()?;
        Ok(Config { defaults, settings })
    }

    fn check_sections(&self, layer: &Table, source: &str) -> Result<(), Error> {
        match layer.keys().find(|key| {
            !self.defaults.contains_key(*key) && !SETTINGS_SECTIONS.contains(&key.as_str())
        }) {
            Some(key) => Err(Error::Parse(format!(
                "{source} has an unknown section `{key}`"
            ))),
            None => Ok(()),
        }
    }

    /// Dotted names of every value the environment or a flag sets. Changing them in the file
    /// has no effect while they are set.
    #[must_use]
    pub fn locked(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        leaf_paths(&self.env, "", &mut paths);
        leaf_paths(&self.flags, "", &mut paths);
        paths
    }
}

fn leaf_paths(table: &Table, prefix: &str, out: &mut BTreeSet<String>) {
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            Value::Table(inner) => leaf_paths(inner, &path, out),
            _ => {
                out.insert(path);
            }
        }
    }
}

/// Copies `over` into `base`, descending into tables so `over` only replaces the keys it has.
pub fn merge(base: &mut Table, over: Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(Value::Table(existing)), Value::Table(inner)) => merge(existing, inner),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// The `FANGST_<SECTION>__<KEY>` variables as a layer. `FANGST_TORRENT__MAX_ACTIVE=3` sets
/// `torrent.max_active`; more `__` go deeper, as in `FANGST_NOTIFIERS__NTFY__TOPIC`. Values that
/// read as a TOML boolean, integer, array or quoted string take that type; anything else is text.
/// Variables without `__`, such as `FANGST_API_KEY`, are not settings and are skipped.
#[must_use]
pub fn env_layer(vars: impl IntoIterator<Item = (String, String)>) -> Table {
    let mut layer = Table::new();
    for (name, raw) in vars {
        let Some(rest) = name.strip_prefix(ENV_PREFIX) else {
            continue;
        };
        let path: Vec<String> = rest
            .split(ENV_SEPARATOR)
            .map(str::to_ascii_lowercase)
            .collect();
        if path.len() < 2 || path.iter().any(String::is_empty) {
            continue;
        }
        let mut nested = Table::new();
        insert_path(&mut nested, &path, env_value(&raw));
        merge(&mut layer, nested);
    }
    layer
}

/// Sets `path` in `table`, creating the tables on the way.
pub fn insert_path(table: &mut Table, path: &[String], value: Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut current = table;
    for key in parents {
        let entry = current
            .entry(key.clone())
            .or_insert_with(|| Value::Table(Table::new()));
        if !entry.is_table() {
            *entry = Value::Table(Table::new());
        }
        let Value::Table(next) = entry else {
            return;
        };
        current = next;
    }
    current.insert(last.clone(), value);
}

fn env_value(raw: &str) -> Value {
    let trimmed = raw.trim();
    let typed = trimmed.starts_with('[')
        || trimmed.starts_with('"')
        || trimmed == "true"
        || trimmed == "false"
        || trimmed.parse::<i64>().is_ok();
    if typed
        && let Ok(mut table) = toml::from_str::<Table>(&format!("v = {trimmed}"))
        && let Some(value) = table.remove("v")
    {
        return value;
    }
    Value::String(raw.to_owned())
}

impl Settings {
    /// # Errors
    /// [`Error::Invalid`] naming the first setting that breaks a rule.
    pub fn validate(&self) -> Result<(), Error> {
        let invalid = |why: String| Err(Error::Invalid(why));
        if let Some(folder) = &self.folders.downloads
            && folder.trim().is_empty()
        {
            return invalid("folders.downloads can't be empty".to_owned());
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for server in &self.servers {
            let name = server.name.trim();
            if name.is_empty() || !one_line(name) {
                return invalid("every server needs a name of one line".to_owned());
            }
            if server.password.is_some() {
                return invalid(format!(
                    "server {name}: passwords are not stored in {FILE_NAME}; set it in the web UI or name an environment variable with password_env"
                ));
            }
            if server.host.trim().is_empty() || server.host.chars().any(char::is_whitespace) {
                return invalid(format!("server {name}: the host is missing or has spaces"));
            }
            if server.port == Some(0) {
                return invalid(format!("server {name}: the port must be 1 to 65535"));
            }
            if let Some(connections) = server.connections
                && !(1..=MAX_CONNECTIONS).contains(&connections)
            {
                return invalid(format!(
                    "server {name}: connections must be 1 to {MAX_CONNECTIONS}"
                ));
            }
            if !ids.insert(server.id.clone()) {
                return invalid(format!("two servers have the id {}", server.id));
            }
            if !names.insert(name.to_lowercase()) {
                return invalid(format!("two servers are named {name}"));
            }
        }
        let mut categories = BTreeSet::new();
        for category in &self.categories {
            let name = category.name.trim();
            if name.is_empty() || !one_line(name) || name.chars().count() > MAX_NAME_CHARS {
                return invalid(format!(
                    "a category name is one line of 1 to {MAX_NAME_CHARS} characters"
                ));
            }
            if !categories.insert(name.to_lowercase()) {
                return invalid(format!("the category {name} is listed twice"));
            }
            if let Some(folder) = &category.folder
                && Path::new(folder)
                    .components()
                    .any(|c| matches!(c, Component::ParentDir))
            {
                return invalid(format!("category {name}: the folder can't contain `..`"));
            }
        }
        Ok(())
    }
}

fn one_line(text: &str) -> bool {
    !text.chars().any(char::is_control)
}

/// Reads the file, or `None` when there is none.
///
/// # Errors
/// When the file can't be read or isn't valid TOML.
pub fn read_file(path: &Path) -> Result<Option<Table>, Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map(Some)
            .map_err(|e| Error::Parse(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io(path.to_owned(), e)),
    }
}

/// Writes the starting file when there is none. Returns whether it did.
///
/// # Errors
/// When the file can't be written.
pub fn ensure_file(path: &Path) -> Result<bool, Error> {
    if path.exists() {
        return Ok(false);
    }
    write_text(path, &format!("{HEADER}{TEMPLATE}"))?;
    Ok(true)
}

/// Replaces the file with `table`, through a temporary file so a crash leaves the old or the new
/// file, never half of one.
///
/// # Errors
/// When the table doesn't serialize or the file can't be written.
pub fn write_file(path: &Path, table: &Table) -> Result<(), Error> {
    let text = toml::to_string(table).map_err(|e| Error::Parse(e.to_string()))?;
    write_text(path, &format!("{HEADER}\n{text}"))
}

fn write_text(path: &Path, text: &str) -> Result<(), Error> {
    let io = |e| Error::Io(path.to_owned(), e);
    let temp = path.with_extension("toml.tmp");
    let mut file = std::fs::File::create(&temp).map_err(io)?;
    file.write_all(text.as_bytes()).map_err(io)?;
    file.sync_all().map_err(io)?;
    drop(file);
    std::fs::rename(&temp, path).map_err(io)
}

/// Puts `settings` into the file's settings sections, leaving tunable overrides alone. Empty
/// sections are left out so the file stays short.
///
/// # Errors
/// Only if a value can't be represented in TOML.
pub fn set_settings(file: &mut Table, settings: &Settings) -> Result<(), Error> {
    let Value::Table(sections) =
        Value::try_from(settings).map_err(|e| Error::Parse(e.to_string()))?
    else {
        return Err(Error::Parse("settings are not a table".to_owned()));
    };
    for name in SETTINGS_SECTIONS {
        file.remove(name);
    }
    for (name, value) in sections {
        let empty = match &value {
            Value::Table(t) => t.values().all(is_empty_value),
            Value::Array(a) => a.is_empty(),
            _ => false,
        };
        if !empty {
            file.insert(name, value);
        }
    }
    Ok(())
}

fn is_empty_value(value: &Value) -> bool {
    match value {
        Value::Table(t) => t.values().all(is_empty_value),
        Value::Array(a) => a.is_empty(),
        Value::Boolean(b) => !b,
        _ => false,
    }
}

/// Sets the tunables in `section` that differ from the built-in defaults and drops the ones that
/// match, so a later change of a default still reaches users who never touched that value.
pub fn set_overrides(layers: &Layers, file: &mut Table, section: &str, values: Table) {
    let defaults = layers.defaults.get(section).and_then(Value::as_table);
    let mut current = file
        .get(section)
        .and_then(Value::as_table)
        .cloned()
        .unwrap_or_default();
    for (key, value) in values {
        if defaults.and_then(|d| d.get(&key)) == Some(&value) {
            current.remove(&key);
        } else {
            current.insert(key, value);
        }
    }
    if current.is_empty() {
        file.remove(section);
    } else {
        file.insert(section.to_owned(), Value::Table(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        toml::from_str(text).unwrap()
    }

    fn layers(file: &str, env: &[(&str, &str)], flags: &str) -> Layers {
        Layers::new(
            table(file),
            env_layer(env.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))),
            table(flags),
        )
        .unwrap()
    }

    #[test]
    fn later_layers_win_key_by_key() {
        let config = layers(
            "[torrent]\nmax_active = 2\nlisten_port = 7000\n",
            &[("FANGST_TORRENT__MAX_ACTIVE", "3")],
            "[torrent]\nlisten_port = 7001\n",
        )
        .resolve()
        .unwrap();
        assert_eq!(config.defaults.torrent.max_active, 3);
        assert_eq!(config.defaults.torrent.listen_port, 7001);
        // Untouched values stay at the built-in default.
        assert!(config.defaults.torrent.dht);
        assert_eq!(config.defaults.notify.timeout_secs, 10);
    }

    #[test]
    fn env_values_take_their_toml_type_or_stay_text() {
        let env = env_layer([
            ("FANGST_TORRENT__DHT".to_owned(), "false".to_owned()),
            ("FANGST_FOLDERS__DOWNLOADS".to_owned(), "/srv/dl".to_owned()),
            (
                "FANGST_NOTIFIERS__NTFY__TOPIC".to_owned(),
                "\"123\"".to_owned(),
            ),
            ("FANGST_API_KEY".to_owned(), "not a setting".to_owned()),
            ("HOME".to_owned(), "/root".to_owned()),
        ]);
        assert_eq!(
            env,
            table(
                "[torrent]\ndht = false\n[folders]\ndownloads = \"/srv/dl\"\n[notifiers.ntfy]\ntopic = \"123\"\n"
            )
        );
    }

    #[test]
    fn providers_take_a_list_or_commas() {
        let config = layers(
            "",
            &[("FANGST_PROVIDERS__ENABLED", "adapter-sabnzbd, notify-ntfy")],
            "",
        )
        .resolve()
        .unwrap();
        assert_eq!(
            config.settings.providers.enabled,
            ["adapter-sabnzbd", "notify-ntfy"]
        );
    }

    #[test]
    fn unknown_sections_and_keys_are_errors() {
        let err = layers("[torrents]\nmax_active = 1\n", &[], "")
            .resolve()
            .unwrap_err();
        assert!(err.to_string().contains("torrents"), "{err}");
        let err = layers("[torrent]\nmax_activ = 1\n", &[], "")
            .resolve()
            .unwrap_err();
        assert!(err.to_string().contains("max_activ"), "{err}");
        let err = layers("", &[("FANGST_NOPE__X", "1")], "")
            .resolve()
            .unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn a_password_in_the_file_is_refused_with_where_it_belongs() {
        let err = layers(
            "[[servers]]\nname = \"Main\"\nhost = \"news.example.com\"\npassword = \"placeholder\"\n",
            &[],
            "",
        )
        .resolve()
        .unwrap_err();
        assert!(err.to_string().contains("password_env"), "{err}");
    }

    #[test]
    fn servers_get_ids_from_names_and_must_be_distinct() {
        let config = layers(
            "[[servers]]\nname = \"Main\"\nhost = \"news.example.com\"\n",
            &[],
            "",
        )
        .resolve()
        .unwrap();
        assert_eq!(config.settings.servers[0].id, "Main");
        assert!(config.settings.servers[0].enabled);
        let twice = "[[servers]]\nname = \"A\"\nhost = \"a\"\n[[servers]]\nname = \"a\"\nhost = \"b\"\nid = \"other\"\n";
        assert!(layers(twice, &[], "").resolve().is_err());
    }

    #[test]
    fn validation_names_the_problem() {
        for (file, needle) in [
            ("[[servers]]\nname = \"A\"\nhost = \"a b\"\n", "host"),
            (
                "[[servers]]\nname = \"A\"\nhost = \"a\"\nport = 0\n",
                "port",
            ),
            (
                "[[servers]]\nname = \"A\"\nhost = \"a\"\nconnections = 0\n",
                "connections",
            ),
            (
                "[[categories]]\nname = \"tv\"\n[[categories]]\nname = \"TV\"\n",
                "twice",
            ),
            ("[[categories]]\nname = \"tv\"\nfolder = \"../out\"\n", ".."),
        ] {
            let err = layers(file, &[], "").resolve().unwrap_err();
            assert!(err.to_string().contains(needle), "{file}: {err}");
        }
    }

    #[test]
    fn locked_lists_what_env_and_flags_set() {
        let locked = layers(
            "[torrent]\nmax_active = 2\n",
            &[("FANGST_TORRENT__MAX_ACTIVE", "3")],
            "[torrent]\nenabled = false\n",
        )
        .locked();
        assert_eq!(
            locked.into_iter().collect::<Vec<_>>(),
            ["torrent.enabled", "torrent.max_active"]
        );
    }

    #[test]
    fn settings_round_trip_through_the_file_without_touching_tunables() {
        let base = layers("[progress]\nrate_window_secs = 9\n", &[], "");
        let mut file = base.file.clone();
        let settings = Settings {
            servers: vec![Server {
                id: "s1".into(),
                name: "Main".into(),
                host: "news.example.com".into(),
                port: Some(119),
                tls: Some(false),
                username: Some("someone".into()),
                password_env: None,
                connections: Some(20),
                priority: 0,
                backfill: false,
                enabled: true,
                password: Some("must not be written".into()),
            }],
            categories: vec![Category {
                name: "tv".into(),
                folder: Some("TV".into()),
            }],
            ..Settings::default()
        };
        set_settings(&mut file, &settings).unwrap();
        let text = toml::to_string(&file).unwrap();
        assert!(!text.contains("must not be written"), "{text}");
        assert!(
            !text.contains("notifiers"),
            "empty sections are left out: {text}"
        );
        let config = base.with_file(file).resolve().unwrap();
        assert_eq!(config.defaults.progress.rate_window_secs, 9);
        assert_eq!(config.settings.servers[0].host, "news.example.com");
        assert_eq!(config.settings.categories, settings.categories);
    }

    #[test]
    fn overrides_equal_to_the_default_are_dropped() {
        let base = layers("[torrent]\nmax_active = 2\n", &[], "");
        let mut file = base.file.clone();
        set_overrides(
            &base,
            &mut file,
            "torrent",
            table("max_active = 5\nlisten_port = 7000\n"),
        );
        assert_eq!(file, table("[torrent]\nlisten_port = 7000\n"));
        set_overrides(&base, &mut file, "torrent", table("listen_port = 6881\n"));
        assert!(file.is_empty());
    }

    #[test]
    fn the_first_file_is_written_once_and_parses() {
        let dir = std::env::temp_dir().join(format!(
            "fangst-config-{}",
            crate::secret::random_token().unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        assert!(ensure_file(&path).unwrap());
        assert!(!ensure_file(&path).unwrap());
        let file = read_file(&path).unwrap().unwrap();
        let config = Layers::new(file, Table::new(), Table::new())
            .unwrap()
            .resolve()
            .unwrap();
        assert_eq!(
            config.settings.folders.downloads_path(&dir),
            dir.join("downloads")
        );
        write_file(&path, &table("[torrent]\nmax_active = 1\n")).unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .starts_with("# Fangst settings")
        );
        assert_eq!(
            read_file(&path).unwrap().unwrap(),
            table("[torrent]\nmax_active = 1\n")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
