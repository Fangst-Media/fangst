//! The daemon's settings: layered from the defaults, `fangst.toml`, `FANGST_*` variables and
//! flags, applied to the running service, and served to `/api/v1/settings`.
//!
//! Passwords and tokens never go into the file. The web UI stores them sealed in `downloads.db`
//! through [`Secrets`]; the file and flags can only name an environment variable holding one.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use fangst_core::config::{self, Config, Layers, Table, Value};
use fangst_dl_api::{
    CategorySettings, ControlError, ControlFuture, DiscordSettings, EnabledAdapters,
    FolderSettings, NotifierSettings, NtfySettings, ProviderSettings, SavedSettings,
    ServerSettings, SettingSource, SettingsDto, TorrentSettings, WebhookSettings,
};
use fangst_downloads::{Notifications, Secrets, Service};
use fangst_nntp::{Pool, ServerConfig};
use fangst_providers::{Notifier, ProviderInfo, ProviderKind};

/// Reads an environment variable; a function so tests can supply their own.
pub type Env = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A usenet server from the environment or the command line rather than the file.
#[derive(Clone)]
pub struct ExtraServer {
    pub source: SettingSource,
    pub host: String,
    pub port: Option<u16>,
    pub tls: Option<bool>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub connections: Option<usize>,
}

impl ExtraServer {
    fn id(&self) -> &'static str {
        match self.source {
            SettingSource::Flags => "flags",
            _ => "environment",
        }
    }
}

/// `FANGST_USENET_HOST` and its companions describe one server without a config file, for
/// containers. The password comes from `FANGST_USENET_PASSWORD`.
pub fn env_server(env: &dyn Fn(&str) -> Option<String>) -> anyhow::Result<Option<ExtraServer>> {
    let Some(host) = env("FANGST_USENET_HOST").filter(|h| !h.trim().is_empty()) else {
        return Ok(None);
    };
    let parse = |name: &str| -> anyhow::Result<Option<u64>> {
        env(name)
            .map(|v| {
                v.trim()
                    .parse::<u64>()
                    .with_context(|| format!("{name} must be a number"))
            })
            .transpose()
    };
    let port = parse("FANGST_USENET_PORT")?
        .map(u16::try_from)
        .transpose()
        .context("FANGST_USENET_PORT must be 1 to 65535")?;
    let connections = parse("FANGST_USENET_CONNECTIONS")?
        .map(usize::try_from)
        .transpose()
        .context("FANGST_USENET_CONNECTIONS is too large")?;
    let tls = env("FANGST_USENET_TLS")
        .map(|v| match v.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(true),
            "false" | "0" | "no" => Ok(false),
            _ => anyhow::bail!("FANGST_USENET_TLS must be true or false"),
        })
        .transpose()?;
    Ok(Some(ExtraServer {
        source: SettingSource::Environment,
        host: host.trim().to_owned(),
        port,
        tls,
        username: env("FANGST_USENET_USERNAME").filter(|u| !u.is_empty()),
        password: env("FANGST_USENET_PASSWORD").filter(|p| !p.is_empty()),
        connections,
    }))
}

/// Where a secret lives in the encrypted store.
fn server_secret(id: &str) -> String {
    format!("server:{id}:password")
}

const WEBHOOK_TOKEN: &str = "notify:webhook:token";
const NTFY_TOKEN: &str = "notify:ntfy:token";
const DISCORD_URL: &str = "notify:discord:webhook_url";

/// A secret a request changes: `Some` sets it, `None` removes it.
type SecretChanges = HashMap<String, Option<String>>;

/// The notifier secrets, looked up before building the notifiers.
#[derive(Default)]
pub struct NotifierSecrets {
    pub webhook_token: Option<String>,
    pub ntfy_token: Option<String>,
    pub discord_url: Option<String>,
}

/// Builds the notifiers `enabled` names from the settings. Fails naming what is missing.
pub fn build_notifiers(
    config: &Config,
    enabled: &[String],
    secrets: &NotifierSecrets,
) -> anyhow::Result<Vec<Arc<dyn Notifier>>> {
    let on = |id: &str| enabled.iter().any(|e| e == id);
    let timeout = config.defaults.notify.timeout();
    let notifiers = &config.settings.notifiers;
    let mut built: Vec<Arc<dyn Notifier>> = Vec::new();
    if on(fangst_notify_webhook::ID) {
        let url = notifiers
            .webhook
            .url
            .clone()
            .context("notify-webhook needs notifiers.webhook.url (--webhook-url)")?;
        built.push(Arc::new(fangst_notify_webhook::WebhookNotifier::new(
            fangst_notify_webhook::Settings {
                url,
                bearer_token: secrets.webhook_token.clone(),
                timeout,
            },
        )?));
    }
    if on(fangst_notify_ntfy::ID) {
        let topic = notifiers
            .ntfy
            .topic
            .clone()
            .context("notify-ntfy needs notifiers.ntfy.topic (--ntfy-topic)")?;
        built.push(Arc::new(fangst_notify_ntfy::NtfyNotifier::new(
            fangst_notify_ntfy::Settings {
                server: notifiers
                    .ntfy
                    .server
                    .clone()
                    .unwrap_or_else(|| config.defaults.notify.ntfy_server.clone()),
                topic,
                token: secrets.ntfy_token.clone(),
                timeout,
            },
        )?));
    }
    if on(fangst_notify_discord::ID) {
        let webhook_url = secrets.discord_url.clone().context(
            "notify-discord needs a webhook URL: set it in the web UI or name a variable with notifiers.discord.webhook_url_env (--discord-webhook-env)",
        )?;
        built.push(Arc::new(fangst_notify_discord::DiscordNotifier::new(
            &fangst_notify_discord::Settings {
                webhook_url,
                timeout,
            },
        )?));
    }
    Ok(built)
}

pub fn notifications(config: &Config, notifiers: Vec<Arc<dyn Notifier>>) -> Notifications {
    Notifications {
        notifiers,
        timeout: config.defaults.notify.timeout(),
        min_free_bytes: config.defaults.disk.min_free_bytes,
        disk_check_interval: config.defaults.disk.check_interval(),
    }
}

/// The torrent engine's settings, or `None` when it is switched off.
pub fn torrent_config(config: &Config, data_dir: &Path) -> Option<fangst_torrent::Config> {
    let torrent = &config.defaults.torrent;
    torrent.enabled.then(|| fangst_torrent::Config {
        listen: (std::net::Ipv4Addr::UNSPECIFIED, torrent.listen_port).into(),
        announce_port: (torrent.announce_port != 0).then_some(torrent.announce_port),
        dht: torrent.dht,
        local_discovery: torrent.local_discovery,
        upnp: torrent.upnp,
        state_dir: Some(data_dir.join("torrent")),
        download_limit: NonZeroU32::new(torrent.download_limit),
        upload_limit: NonZeroU32::new(torrent.upload_limit),
    })
}

/// How many connections each server worker set opens: the most any one server allows, since the
/// download engine caps each server at its own count.
pub fn usenet_options(
    config: &Config,
    servers: &[fangst_usenet::Server],
) -> fangst_usenet::Options {
    fangst_usenet::Options {
        connections: servers
            .iter()
            .map(|s| s.pool.config().max_connections)
            .max()
            .unwrap_or(config.defaults.usenet.connections),
        pipeline_depth: config.defaults.usenet.pipeline,
        retries: config.defaults.usenet.retries,
    }
}

/// Where server passwords and notifier tokens come from, and the server outside the file.
pub struct Sources {
    pub secrets: Secrets,
    pub env: Env,
    pub extra_server: Option<ExtraServer>,
    /// On whatever the settings say: required by the daemon or enabled by a flag.
    pub required: Vec<String>,
}

impl Sources {
    /// Provider ids that are on: the required ones, then those the settings enable.
    #[must_use]
    pub fn enabled(&self, config: &Config) -> Vec<String> {
        enabled_ids(&self.required, config)
    }

    async fn secret(
        &self,
        name: &str,
        changes: &SecretChanges,
    ) -> Result<Option<String>, ControlError> {
        if let Some(change) = changes.get(name) {
            return Ok(change.clone());
        }
        self.secrets.get(name).await.map_err(internal)
    }

    /// A secret from the variable a setting names, else from the store or this request.
    async fn secret_or_env(
        &self,
        env_name: Option<&str>,
        name: &str,
        changes: &SecretChanges,
    ) -> Result<Option<String>, ControlError> {
        match env_name {
            Some(var) => (self.env)(var).map(Some).ok_or_else(|| {
                ControlError::Invalid(format!("environment variable {var} is not set"))
            }),
            None => self.secret(name, changes).await,
        }
    }

    pub async fn notifier_secrets(
        &self,
        config: &Config,
        changes: &SecretChanges,
    ) -> Result<NotifierSecrets, ControlError> {
        let n = &config.settings.notifiers;
        let enabled = self.enabled(config);
        let on = |id: &str| enabled.iter().any(|e| e == id);
        // Only enabled notifiers need their variables set; a disabled one may name a missing one.
        let lookup = async |on: bool, var: Option<&str>, name: &str| {
            if on {
                self.secret_or_env(var, name, changes).await
            } else {
                Ok(None)
            }
        };
        Ok(NotifierSecrets {
            webhook_token: lookup(
                on(fangst_notify_webhook::ID),
                n.webhook.token_env.as_deref(),
                WEBHOOK_TOKEN,
            )
            .await?,
            ntfy_token: lookup(
                on(fangst_notify_ntfy::ID),
                n.ntfy.token_env.as_deref(),
                NTFY_TOKEN,
            )
            .await?,
            discord_url: lookup(
                on(fangst_notify_discord::ID),
                n.discord.webhook_url_env.as_deref(),
                DISCORD_URL,
            )
            .await?,
        })
    }

    /// The enabled servers in the order they are tried: one from the environment or flags first,
    /// then the file's by priority.
    pub async fn usenet_servers(
        &self,
        config: &Config,
        changes: &SecretChanges,
    ) -> Result<Vec<fangst_usenet::Server>, ControlError> {
        let defaults = &config.defaults.usenet;
        let mut servers = Vec::new();
        if let Some(extra) = &self.extra_server {
            servers.push(server(
                extra.id().to_owned(),
                with_tls(
                    ServerConfig {
                        host: extra.host.clone(),
                        port: extra.port.unwrap_or(defaults.port),
                        tls: None,
                        credentials: extra
                            .username
                            .clone()
                            .map(|u| (u, extra.password.clone().unwrap_or_default())),
                        max_connections: extra.connections.unwrap_or(defaults.connections),
                        connect_timeout: defaults.connect_timeout(),
                        command_timeout: defaults.command_timeout(),
                    },
                    extra.tls.unwrap_or(defaults.tls),
                )?,
                false,
            ));
        }
        let mut file: Vec<&config::Server> = config
            .settings
            .servers
            .iter()
            .filter(|s| s.enabled)
            .collect();
        file.sort_by_key(|s| s.priority);
        for entry in file {
            let password = self
                .secret_or_env(
                    entry.password_env.as_deref(),
                    &server_secret(&entry.id),
                    changes,
                )
                .await?;
            servers.push(server(
                entry.name.clone(),
                with_tls(
                    ServerConfig {
                        host: entry.host.clone(),
                        port: entry.port.unwrap_or(defaults.port),
                        tls: None,
                        credentials: entry
                            .username
                            .clone()
                            .map(|u| (u, password.unwrap_or_default())),
                        max_connections: entry.connections.unwrap_or(defaults.connections),
                        connect_timeout: defaults.connect_timeout(),
                        command_timeout: defaults.command_timeout(),
                    },
                    entry.tls.unwrap_or(defaults.tls),
                )?,
                entry.backfill,
            ));
        }
        Ok(servers)
    }
}

/// Everything the settings routes need, and what applies a change to the running daemon.
pub struct Daemon {
    data_dir: PathBuf,
    file_path: PathBuf,
    layers: tokio::sync::Mutex<Layers>,
    /// What the daemon started with, to tell which changes wait for a restart.
    started: Config,
    sources: Sources,
    service: Service,
    adapters: EnabledAdapters,
    /// Adapters and post-processors compiled in.
    providers: Vec<ProviderInfo>,
}

pub struct DaemonParts {
    pub data_dir: PathBuf,
    pub layers: Layers,
    pub sources: Sources,
    pub service: Service,
    pub adapters: EnabledAdapters,
    pub providers: Vec<ProviderInfo>,
}

impl Daemon {
    /// # Errors
    /// When the layers don't resolve, which the caller already checked at start.
    pub fn new(parts: DaemonParts) -> anyhow::Result<Self> {
        Ok(Self {
            file_path: parts.data_dir.join(config::FILE_NAME),
            data_dir: parts.data_dir,
            started: parts.layers.resolve()?,
            layers: tokio::sync::Mutex::new(parts.layers),
            sources: parts.sources,
            service: parts.service,
            adapters: parts.adapters,
            providers: parts.providers,
        })
    }

    fn enabled(&self, config: &Config) -> Vec<String> {
        self.sources.enabled(config)
    }

    /// Creates the configured categories and points their jobs at their folders.
    pub async fn apply_categories(
        &self,
        config: &Config,
        removed: &[String],
    ) -> Result<(), ControlError> {
        let store = self.service.store();
        for name in removed {
            store.delete_category(name).await.map_err(internal)?;
        }
        let mut folders = HashMap::new();
        for category in &config.settings.categories {
            store
                .create_category(&category.name)
                .await
                .map_err(|e| ControlError::Invalid(e.to_string()))?;
            if let Some(folder) = &category.folder {
                folders.insert(category.name.clone(), PathBuf::from(folder));
            }
        }
        store.set_category_folders(folders);
        Ok(())
    }

    /// Applies what can change while running.
    async fn apply(
        &self,
        config: &Config,
        servers: Vec<fangst_usenet::Server>,
        notifiers: Vec<Arc<dyn Notifier>>,
        removed_categories: &[String],
    ) -> Result<(), ControlError> {
        let options = usenet_options(config, &servers);
        self.service.set_usenet(servers, options);
        self.service
            .set_notifications(notifications(config, notifiers));
        self.apply_categories(config, removed_categories).await?;
        let enabled = self.enabled(config);
        self.adapters.set(
            self.providers
                .iter()
                .filter(|p| p.kind == ProviderKind::ApiAdapter && enabled.contains(&p.id))
                .map(|p| p.id.clone()),
        );
        Ok(())
    }

    fn restart_reasons(&self, new: &Config) -> Vec<String> {
        let old = &self.started;
        let mut reasons = Vec::new();
        if old.settings.folders.downloads_path(&self.data_dir)
            != new.settings.folders.downloads_path(&self.data_dir)
        {
            reasons.push("the downloads folder".to_owned());
        }
        if old.defaults.torrent != new.defaults.torrent {
            reasons.push("torrent settings".to_owned());
        }
        if old.defaults.http != new.defaults.http {
            reasons.push("the listen address or cookie setting".to_owned());
        }
        if old.defaults.progress != new.defaults.progress {
            reasons.push("progress reporting".to_owned());
        }
        let post_processors = |config: &Config| -> HashSet<String> {
            let enabled = self.enabled(config);
            self.providers
                .iter()
                .filter(|p| p.kind == ProviderKind::PostProcessor && enabled.contains(&p.id))
                .map(|p| p.id.clone())
                .collect()
        };
        if old.defaults.extract != new.defaults.extract
            || old.defaults.safety != new.defaults.safety
            || post_processors(old) != post_processors(new)
        {
            reasons.push("post-processing".to_owned());
        }
        reasons
    }

    async fn has_secret(
        &self,
        env_name: Option<&String>,
        name: &str,
    ) -> Result<bool, ControlError> {
        if env_name.is_some() {
            return Ok(true);
        }
        Ok(self
            .sources
            .secrets
            .get(name)
            .await
            .map_err(internal)?
            .is_some())
    }

    async fn server_dtos(&self, config: &Config) -> Result<Vec<ServerSettings>, ControlError> {
        let defaults = &config.defaults.usenet;
        let mut servers = Vec::new();
        if let Some(extra) = &self.sources.extra_server {
            servers.push(ServerSettings {
                id: Some(extra.id().to_owned()),
                name: extra.host.clone(),
                host: extra.host.clone(),
                port: extra.port.unwrap_or(defaults.port),
                tls: extra.tls.unwrap_or(defaults.tls),
                username: extra.username.clone(),
                password: None,
                has_password: extra.password.is_some(),
                password_env: None,
                connections: extra.connections.unwrap_or(defaults.connections),
                priority: i64::MIN,
                backfill: false,
                enabled: true,
                source: extra.source,
            });
        }
        for entry in &config.settings.servers {
            servers.push(ServerSettings {
                id: Some(entry.id.clone()),
                name: entry.name.clone(),
                host: entry.host.clone(),
                port: entry.port.unwrap_or(defaults.port),
                tls: entry.tls.unwrap_or(defaults.tls),
                username: entry.username.clone(),
                password: None,
                has_password: self
                    .has_secret(entry.password_env.as_ref(), &server_secret(&entry.id))
                    .await?,
                password_env: entry.password_env.clone(),
                connections: entry.connections.unwrap_or(defaults.connections),
                priority: entry.priority,
                backfill: entry.backfill,
                enabled: entry.enabled,
                source: SettingSource::File,
            });
        }
        Ok(servers)
    }

    async fn notifier_dto(&self, config: &Config) -> Result<NotifierSettings, ControlError> {
        let enabled = self.enabled(config);
        let on = |id: &str| enabled.iter().any(|e| e == id);
        let n = &config.settings.notifiers;
        Ok(NotifierSettings {
            webhook: WebhookSettings {
                enabled: on(fangst_notify_webhook::ID),
                url: n.webhook.url.clone(),
                token: None,
                has_token: self
                    .has_secret(n.webhook.token_env.as_ref(), WEBHOOK_TOKEN)
                    .await?,
                token_env: n.webhook.token_env.clone(),
            },
            ntfy: NtfySettings {
                enabled: on(fangst_notify_ntfy::ID),
                server: n.ntfy.server.clone(),
                topic: n.ntfy.topic.clone(),
                token: None,
                has_token: self
                    .has_secret(n.ntfy.token_env.as_ref(), NTFY_TOKEN)
                    .await?,
                token_env: n.ntfy.token_env.clone(),
            },
            discord: DiscordSettings {
                enabled: on(fangst_notify_discord::ID),
                webhook_url: None,
                has_webhook_url: self
                    .has_secret(n.discord.webhook_url_env.as_ref(), DISCORD_URL)
                    .await?,
                webhook_url_env: n.discord.webhook_url_env.clone(),
            },
        })
    }

    async fn dto(&self, layers: &Layers, config: &Config) -> Result<SettingsDto, ControlError> {
        let enabled = self.enabled(config);
        Ok(SettingsDto {
            folders: FolderSettings {
                downloads: config
                    .settings
                    .folders
                    .downloads
                    .clone()
                    .unwrap_or_else(|| config::DEFAULT_DOWNLOADS.to_owned()),
                media: config.settings.folders.media.clone(),
            },
            servers: self.server_dtos(config).await?,
            categories: config
                .settings
                .categories
                .iter()
                .map(|c| CategorySettings {
                    name: c.name.clone(),
                    folder: c.folder.clone(),
                })
                .collect(),
            notifiers: self.notifier_dto(config).await?,
            torrent: torrent_dto(config),
            providers: self
                .providers
                .iter()
                .map(|p| ProviderSettings {
                    id: p.id.clone(),
                    kind: match p.kind {
                        ProviderKind::ApiAdapter => "api_adapter",
                        ProviderKind::PostProcessor => "post_processor",
                        ProviderKind::Notifier => "notifier",
                    }
                    .to_owned(),
                    description: p.description.clone(),
                    enabled: enabled.contains(&p.id),
                    required: self.sources.required.contains(&p.id),
                })
                .collect(),
            locked: layers.locked().into_iter().collect(),
        })
    }

    /// The settings a request asks for, its torrent section, and the secrets it sets or removes.
    fn requested(
        &self,
        current: &Config,
        dto: SettingsDto,
    ) -> Result<(config::Settings, Table, SecretChanges), ControlError> {
        let mut changes = SecretChanges::new();
        let mut settings = current.settings.clone();
        settings.folders = config::Folders {
            downloads: Some(dto.folders.downloads.trim().to_owned()),
            media: blank_to_none(dto.folders.media),
        };
        settings.servers = servers_from_dto(dto.servers, &current.settings, &mut changes)?;
        settings.categories = dto
            .categories
            .into_iter()
            .map(|c| config::Category {
                name: c.name.trim().to_owned(),
                folder: blank_to_none(c.folder),
            })
            .collect();
        let n = dto.notifiers;
        let notifier_ids = [
            (n.webhook.enabled, fangst_notify_webhook::ID),
            (n.ntfy.enabled, fangst_notify_ntfy::ID),
            (n.discord.enabled, fangst_notify_discord::ID),
        ];
        settings.notifiers = notifiers_from_dto(n, &mut changes);
        let known: HashSet<&str> = self.providers.iter().map(|p| p.id.as_str()).collect();
        settings.providers.enabled = dto
            .providers
            .iter()
            .filter(|p| p.enabled && known.contains(p.id.as_str()))
            .map(|p| p.id.clone())
            .chain(
                notifier_ids
                    .iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, id)| (*id).to_owned()),
            )
            .filter(|id| !self.sources.required.contains(id))
            .collect();
        Ok((settings, torrent_table(&dto.torrent)?, changes))
    }

    async fn save(&self, dto: SettingsDto) -> Result<SavedSettings, ControlError> {
        let mut layers = self.layers.lock().await;
        let current = layers.resolve().map_err(internal)?;
        let (settings, torrent, changes) = self.requested(&current, dto)?;
        let mut file = layers.file.clone();
        config::set_settings(&mut file, &settings).map_err(invalid)?;
        config::set_overrides(&layers, &mut file, "torrent", torrent);
        keep_locked(&layers, &mut file)?;

        let next = layers.with_file(file);
        let config = next.resolve().map_err(invalid)?;
        if config.defaults.torrent.max_active == 0 {
            return Err(invalid("torrent.max_active must be at least 1"));
        }
        // Everything is built before anything is stored, so a bad request changes nothing.
        let servers = self.sources.usenet_servers(&config, &changes).await?;
        let secrets = self.sources.notifier_secrets(&config, &changes).await?;
        let notifiers = build_notifiers(&config, &self.enabled(&config), &secrets)
            .map_err(|e| invalid(format!("{e:#}")))?;

        for (name, value) in &changes {
            match value {
                Some(value) => self.sources.secrets.put(name, value).await,
                None => self.sources.secrets.delete(name).await,
            }
            .map_err(internal)?;
        }
        config::write_file(&self.file_path, &next.file).map_err(internal)?;
        let removed = removed_categories(&current, &config);
        *layers = next;
        self.apply(&config, servers, notifiers, &removed).await?;
        let reasons = self.restart_reasons(&config);
        Ok(SavedSettings {
            settings: self.dto(&layers, &config).await?,
            restart_required: !reasons.is_empty(),
            restart_reasons: reasons,
        })
    }
}

fn torrent_dto(config: &Config) -> TorrentSettings {
    let torrent = &config.defaults.torrent;
    TorrentSettings {
        enabled: torrent.enabled,
        listen_port: torrent.listen_port,
        announce_port: (torrent.announce_port != 0).then_some(torrent.announce_port),
        dht: torrent.dht,
        local_discovery: torrent.local_discovery,
        upnp: torrent.upnp,
        download_limit: (torrent.download_limit != 0).then_some(torrent.download_limit),
        upload_limit: (torrent.upload_limit != 0).then_some(torrent.upload_limit),
        max_active: torrent.max_active,
    }
}

fn torrent_table(t: &TorrentSettings) -> Result<Table, ControlError> {
    let mut torrent = Table::new();
    for (key, on) in [
        ("enabled", t.enabled),
        ("dht", t.dht),
        ("local_discovery", t.local_discovery),
        ("upnp", t.upnp),
    ] {
        torrent.insert(key.into(), on.into());
    }
    let max_active =
        i64::try_from(t.max_active).map_err(|_| invalid("torrent.max_active is too large"))?;
    for (key, value) in [
        ("listen_port", i64::from(t.listen_port)),
        ("announce_port", i64::from(t.announce_port.unwrap_or(0))),
        ("download_limit", i64::from(t.download_limit.unwrap_or(0))),
        ("upload_limit", i64::from(t.upload_limit.unwrap_or(0))),
        ("max_active", max_active),
    ] {
        torrent.insert(key.into(), value.into());
    }
    Ok(torrent)
}

/// The file's servers from a request. A server without an id is new; one left out is removed,
/// and so is its stored password.
fn servers_from_dto(
    requested: Vec<ServerSettings>,
    current: &config::Settings,
    changes: &mut SecretChanges,
) -> Result<Vec<config::Server>, ControlError> {
    let mut servers = Vec::new();
    for entry in requested
        .into_iter()
        .filter(|s| s.source == SettingSource::File)
    {
        let id = match entry.id.filter(|id| !id.trim().is_empty()) {
            Some(id) => id,
            None => new_id()?,
        };
        if let Some(password) = entry.password {
            changes.insert(server_secret(&id), Some(password).filter(|p| !p.is_empty()));
        }
        servers.push(config::Server {
            id,
            name: entry.name.trim().to_owned(),
            host: entry.host.trim().to_owned(),
            port: Some(entry.port),
            tls: Some(entry.tls),
            username: blank_to_none(entry.username),
            password_env: blank_to_none(entry.password_env),
            connections: Some(entry.connections),
            priority: entry.priority,
            backfill: entry.backfill,
            enabled: entry.enabled,
            password: None,
        });
    }
    let kept: HashSet<&str> = servers.iter().map(|s| s.id.as_str()).collect();
    for old in &current.servers {
        if !kept.contains(old.id.as_str()) {
            changes.insert(server_secret(&old.id), None);
        }
    }
    Ok(servers)
}

fn notifiers_from_dto(n: NotifierSettings, changes: &mut SecretChanges) -> config::Notifiers {
    for (name, value) in [
        (WEBHOOK_TOKEN, n.webhook.token),
        (NTFY_TOKEN, n.ntfy.token),
        (DISCORD_URL, n.discord.webhook_url),
    ] {
        if let Some(value) = value {
            changes.insert(name.to_owned(), Some(value).filter(|v| !v.is_empty()));
        }
    }
    config::Notifiers {
        webhook: config::Webhook {
            url: blank_to_none(n.webhook.url),
            token_env: blank_to_none(n.webhook.token_env),
        },
        ntfy: config::Ntfy {
            server: blank_to_none(n.ntfy.server),
            topic: blank_to_none(n.ntfy.topic),
            token_env: blank_to_none(n.ntfy.token_env),
        },
        discord: config::Discord {
            webhook_url_env: blank_to_none(n.discord.webhook_url_env),
        },
    }
}

fn removed_categories(old: &Config, new: &Config) -> Vec<String> {
    old.settings
        .categories
        .iter()
        .filter(|o| {
            !new.settings
                .categories
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(&o.name))
        })
        .map(|c| c.name.clone())
        .collect()
}

impl fangst_dl_api::Control for Daemon {
    fn settings(&self) -> ControlFuture<'_, Result<SettingsDto, ControlError>> {
        Box::pin(async move {
            let layers = self.layers.lock().await;
            let config = layers.resolve().map_err(internal)?;
            self.dto(&layers, &config).await
        })
    }

    fn save_settings(
        &self,
        settings: SettingsDto,
    ) -> ControlFuture<'_, Result<SavedSettings, ControlError>> {
        Box::pin(self.save(settings))
    }
}

/// A value the environment or a flag sets can't be changed through the file. The request may
/// carry it back unchanged, which is what the web UI does; the file keeps its own value then.
fn keep_locked(layers: &Layers, file: &mut Table) -> Result<(), ControlError> {
    let effective = layers.merged();
    let mut requested = layers.defaults.clone();
    config::merge(&mut requested, file.clone());
    for path in layers.locked() {
        let keys: Vec<String> = path.split('.').map(str::to_owned).collect();
        let section = keys.first().map_or("", String::as_str);
        // Provider lists from the environment show up as required providers instead, and
        // sections a request doesn't carry can't have been changed by it.
        let carried = section == "torrent" || config::SETTINGS_SECTIONS.contains(&section);
        if section == "providers" || !carried {
            restore(file, &layers.file, &keys);
            continue;
        }
        if lookup(&requested, &keys) != lookup(&effective, &keys) {
            return Err(ControlError::Invalid(format!(
                "{path} is set by an environment variable or command-line flag; change it there"
            )));
        }
        restore(file, &layers.file, &keys);
    }
    Ok(())
}

fn lookup<'a>(table: &'a Table, keys: &[String]) -> Option<&'a Value> {
    let (last, parents) = keys.split_last()?;
    let mut current = table;
    for key in parents {
        current = current.get(key)?.as_table()?;
    }
    current.get(last)
}

/// Puts the old file's value at `keys` back into `file`, or removes it when the old file had none.
fn restore(file: &mut Table, old: &Table, keys: &[String]) {
    if let Some(value) = lookup(old, keys) {
        config::insert_path(file, keys, value.clone());
        return;
    }
    let Some((last, parents)) = keys.split_last() else {
        return;
    };
    let mut current = file;
    for key in parents {
        match current.get_mut(key) {
            Some(Value::Table(next)) => current = next,
            _ => return,
        }
    }
    current.remove(last);
}

#[must_use]
pub fn enabled_ids(required: &[String], config: &Config) -> Vec<String> {
    let mut enabled = required.to_vec();
    for id in &config.settings.providers.enabled {
        if !enabled.contains(id) {
            enabled.push(id.clone());
        }
    }
    enabled
}

fn server(name: String, config: ServerConfig, backfill: bool) -> fangst_usenet::Server {
    fangst_usenet::Server {
        name,
        pool: Arc::new(Pool::new(config)),
        backfill,
    }
}

/// Adds the system's root certificates when `on`; plain TCP otherwise.
fn with_tls(mut config: ServerConfig, on: bool) -> Result<ServerConfig, ControlError> {
    if on {
        config.tls = Some(fangst_nntp::native_tls_config().map_err(internal)?);
    }
    Ok(config)
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

fn new_id() -> Result<String, ControlError> {
    let token = fangst_core::secret::random_token().map_err(internal)?;
    Ok(token.chars().take(12).collect())
}

#[allow(clippy::needless_pass_by_value)] // Taken by value so it fits `map_err` directly.
fn invalid(e: impl ToString) -> ControlError {
    ControlError::Invalid(e.to_string())
}

#[allow(clippy::needless_pass_by_value)] // Taken by value so it fits `map_err` directly.
fn internal(e: impl ToString) -> ControlError {
    ControlError::Internal(e.to_string())
}
