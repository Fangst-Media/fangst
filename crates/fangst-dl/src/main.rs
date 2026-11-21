use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use fangst_core::config::{self, Layers, Table, Value};
use fangst_dl_api::SettingSource;
use fangst_nntp::{Pool, ServerConfig};
use fangst_usenet::Progress;

mod settings;

#[derive(Parser)]
#[command(name = "fangst-dl", version, about = "The Fangst download daemon")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the daemon: the job queue and the HTTP API.
    Serve(Box<ServeArgs>),
    /// Download the files of one NZB from one usenet server, then exit.
    Get(GetArgs),
    /// Write or check the committed OpenAPI document.
    Openapi(OpenapiArgs),
    /// Set a user's password from this machine, for a lost password or the first one. The user is
    /// signed out everywhere. Safe to run while the daemon is running.
    ResetPassword(ResetPasswordArgs),
}

#[derive(clap::Args)]
struct ResetPasswordArgs {
    /// The daemon's data folder, holding downloads.db and master.key.
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,
    #[arg(long, default_value = fangst_downloads::FIRST_USER)]
    user: String,
    /// Name of the environment variable holding the new password. Without it the password is read
    /// from the first line of standard input. It is never taken as an argument, which would leave
    /// it in shell history and process listings.
    #[arg(long)]
    password_env: Option<String>,
}

/// One usenet server from the command line, tried before the servers in the config file.
/// Values left out come from `[usenet]` in the defaults.
#[derive(clap::Args)]
struct UsenetArgs {
    #[arg(long)]
    host: Option<String>,
    /// Defaults to `usenet.port`.
    #[arg(long)]
    port: Option<u16>,
    /// Connect without TLS (usually port 119).
    #[arg(long)]
    no_tls: bool,
    #[arg(long)]
    user: Option<String>,
    /// Name of the environment variable holding the password, so it stays out of shell history.
    #[arg(long, requires = "user")]
    password_env: Option<String>,
    /// Defaults to `usenet.connections`.
    #[arg(long)]
    connections: Option<usize>,
    /// Requests each connection keeps in flight, for every server. Defaults to `usenet.pipeline`.
    #[arg(long)]
    pipeline: Option<usize>,
    /// Seconds to wait for DNS, TCP and TLS when opening a connection.
    #[arg(long)]
    connect_timeout: Option<u64>,
    /// Seconds to wait for any single reply; large articles on slow links need a generous value.
    #[arg(long)]
    command_timeout: Option<u64>,
}

impl UsenetArgs {
    fn server(&self) -> anyhow::Result<Option<settings::ExtraServer>> {
        let Some(host) = &self.host else {
            return Ok(None);
        };
        let password = match (&self.user, &self.password_env) {
            (Some(_), Some(var)) => Some(
                std::env::var(var)
                    .with_context(|| format!("environment variable {var} is not set"))?,
            ),
            (Some(_), None) => bail!("--user needs --password-env"),
            _ => None,
        };
        Ok(Some(settings::ExtraServer {
            source: SettingSource::Flags,
            host: host.clone(),
            port: self.port,
            tls: self.no_tls.then_some(false),
            username: self.user.clone(),
            password,
            connections: self.connections,
        }))
    }

    fn layer(&self, layer: &mut Table) {
        let to_int = |n: u64| i64::try_from(n).unwrap_or(i64::MAX);
        for (key, value) in [
            ("pipeline", self.pipeline.map(|n| to_int(n as u64))),
            ("connect_timeout_secs", self.connect_timeout.map(to_int)),
            ("command_timeout_secs", self.command_timeout.map(to_int)),
        ] {
            if let Some(value) = value {
                set(layer, &["usenet", key], value.into());
            }
        }
    }
}

fn set(layer: &mut Table, path: &[&str], value: Value) {
    let path: Vec<String> = path.iter().map(|&k| k.to_owned()).collect();
    config::insert_path(layer, &path, value);
}

#[derive(clap::Args)]
struct ServeArgs {
    /// Folder for downloads.db, fangst.toml and, unless the settings say otherwise, the downloads.
    /// Defaults to `FANGST_DATA_DIR`, else `data`.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Address the API listens on; port 0 picks a free port. Defaults to `http.listen`.
    #[arg(long)]
    listen: Option<String>,
    /// Name of the environment variable holding the API key. It becomes the first user's key and
    /// replaces the one set at the previous start.
    #[arg(long, default_value = "FANGST_API_KEY")]
    api_key_env: String,
    /// Turn on an optional provider (post-processor, notifier, API adapter) by its id, on top of
    /// the ones the settings enable. Repeatable.
    #[arg(long = "enable", value_name = "ID")]
    enabled_providers: Vec<String>,
    /// Mark the session cookie Secure; use when the API is only reached over HTTPS.
    #[arg(long)]
    secure_cookie: bool,
    #[command(flatten)]
    usenet: UsenetArgs,
    #[command(flatten)]
    torrent: TorrentArgs,
    #[command(flatten)]
    notify: NotifyArgs,
}

impl ServeArgs {
    /// The flags as the top settings layer.
    fn layer(&self) -> Table {
        let mut layer = Table::new();
        if let Some(listen) = &self.listen {
            set(&mut layer, &["http", "listen"], listen.clone().into());
        }
        if self.secure_cookie {
            set(&mut layer, &["http", "secure_cookie"], true.into());
        }
        self.usenet.layer(&mut layer);
        self.torrent.layer(&mut layer);
        self.notify.layer(&mut layer);
        layer
    }
}

/// The torrent engine. On unless `--no-torrent`; its state lives in `<data-dir>/torrent`.
/// Each flag overrides the matching `[torrent]` setting.
#[derive(clap::Args)]
#[allow(clippy::struct_excessive_bools)] // Each is an independent command-line switch.
struct TorrentArgs {
    /// Run without the torrent engine; torrent jobs can still be added and wait in the queue.
    #[arg(long)]
    no_torrent: bool,
    /// TCP port peers connect to; 0 picks a free one at every start. Defaults to
    /// `torrent.listen_port`.
    #[arg(long, value_name = "PORT")]
    torrent_port: Option<u16>,
    /// Port told to trackers and the DHT when peers reach this machine on another one, such as a
    /// port forwarded by a VPN.
    #[arg(long, value_name = "PORT")]
    torrent_announce_port: Option<u16>,
    /// Don't look for peers in the DHT, the tracker-less peer network.
    #[arg(long)]
    no_dht: bool,
    /// Don't look for peers on the local network by multicast.
    #[arg(long)]
    no_local_discovery: bool,
    /// Don't ask the router to forward the torrent port.
    #[arg(long)]
    no_upnp: bool,
    /// Download limit across all torrents, in bytes per second.
    #[arg(long, value_name = "BYTES_PER_SEC")]
    torrent_download_limit: Option<NonZeroU32>,
    /// Upload limit across all torrents, in bytes per second.
    #[arg(long, value_name = "BYTES_PER_SEC")]
    torrent_upload_limit: Option<NonZeroU32>,
    /// Torrent jobs downloading at once. Defaults to `torrent.max_active`.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
    max_active_torrents: Option<u32>,
}

impl TorrentArgs {
    fn layer(&self, layer: &mut Table) {
        let mut put = |key: &str, value: Value| set(layer, &["torrent", key], value);
        for (off, key) in [
            (self.no_torrent, "enabled"),
            (self.no_dht, "dht"),
            (self.no_local_discovery, "local_discovery"),
            (self.no_upnp, "upnp"),
        ] {
            if off {
                put(key, false.into());
            }
        }
        for (value, key) in [
            (self.torrent_port.map(i64::from), "listen_port"),
            (self.torrent_announce_port.map(i64::from), "announce_port"),
            (
                self.torrent_download_limit.map(|n| i64::from(n.get())),
                "download_limit",
            ),
            (
                self.torrent_upload_limit.map(|n| i64::from(n.get())),
                "upload_limit",
            ),
            (self.max_active_torrents.map(i64::from), "max_active"),
        ] {
            if let Some(value) = value {
                put(key, value.into());
            }
        }
    }
}

#[derive(clap::Args)]
struct GetArgs {
    /// The NZB file to download.
    nzb: PathBuf,
    /// Folder the files are written to; created if missing.
    #[arg(long, short)]
    out: PathBuf,
    #[command(flatten)]
    usenet: UsenetArgs,
}

#[derive(clap::Args)]
struct OpenapiArgs {
    /// Path of the committed document.
    path: PathBuf,
    /// Fail if the file differs from what the code generates, instead of writing it.
    #[arg(long)]
    check: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("error: could not start the async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = match cli.command {
        Command::Serve(args) => runtime.block_on(serve(*args)),
        Command::Get(args) => runtime.block_on(get(args)),
        Command::Openapi(args) => openapi(&args),
        Command::ResetPassword(args) => runtime.block_on(reset_password(args)),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn startup(e: fangst_dl_api::ControlError) -> anyhow::Error {
    match e {
        fangst_dl_api::ControlError::Invalid(why) | fangst_dl_api::ControlError::Internal(why) => {
            anyhow::anyhow!(why)
        }
    }
}

async fn serve(args: ServeArgs) -> anyhow::Result<ExitCode> {
    let env: settings::Env = Arc::new(|name| std::env::var(name).ok());
    let api_key = api_key(&args)?;
    let (data_dir, layers, config) = load_settings(&args, &env)?;
    let store = fangst_downloads::Store::open(&data_dir.join("downloads.db")).await?;
    let master_key = Arc::new(fangst_core::secret::MasterKey::load_or_create(
        &data_dir.join("master.key"),
    )?);
    let auth = fangst_downloads::Auth::new(&store, Arc::clone(&master_key)).await?;
    let owner = accounts(&auth, api_key.as_deref(), &env).await?;
    let extra_server = match args.usenet.server()? {
        Some(server) => Some(server),
        None => settings::env_server(&*env)?,
    };
    let sources = settings::Sources {
        secrets: fangst_downloads::Secrets::new(&store, master_key),
        env,
        extra_server,
        required: with_required_providers(&args.enabled_providers),
    };

    let Providers {
        registry,
        infos,
        adapters,
    } = providers(&config, &sources).await?;
    let servers = sources
        .usenet_servers(&config, &HashMap::new())
        .await
        .map_err(startup)?;
    let has_server = !servers.is_empty();
    let runner = fangst_downloads::Runner {
        store,
        options: settings::usenet_options(&config, &servers),
        // Without a server the queue still accepts jobs; they wait until one is configured.
        servers,
        root: config.settings.folders.downloads_path(&data_dir),
        post_processors: registry.post_processors.clone(),
    };
    let mut service = fangst_downloads::Service::new(runner)
        .with_notifications(settings::notifications(&config, registry.notifiers.clone()))
        .with_progress(config.defaults.progress.clone())
        .with_torrent_limit(config.defaults.torrent.max_active);
    let engine = start_torrents(&config, &data_dir).await?;
    if let Some(engine) = &engine {
        service = service.with_torrents(engine.clone());
    }

    let enabled_adapters =
        fangst_dl_api::EnabledAdapters::new(registry.adapters.iter().map(|a| a.info().id));
    let daemon = settings::Daemon::new(settings::DaemonParts {
        data_dir,
        layers,
        sources,
        service: service.clone(),
        adapters: enabled_adapters.clone(),
        providers: infos,
        owner_from_env: owner.from_env,
    })?;
    daemon
        .apply_categories(&config, &[])
        .await
        .map_err(startup)?;

    let listen = &config.defaults.http.listen;
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("listening on {listen}"))?;
    announce(
        listener.local_addr()?,
        has_server,
        engine.as_ref(),
        &registry,
    );
    if let Some(token) = &owner.setup_token {
        println!(
            "setup: open the web UI and enter this setup token to choose the owner's password: {token}"
        );
    }

    let app = fangst_dl_api::app(
        service.clone(),
        auth,
        fangst_dl_api::Options {
            auth: fangst_dl_api::AuthSettings {
                secure_cookie: config.defaults.http.secure_cookie,
                ..fangst_dl_api::AuthSettings::default()
            },
            fetch: fangst_dl_api::FetchLimits::default(),
            adapters,
            enabled_adapters,
            control: Some(Arc::new(daemon)),
            setup_token: owner.setup_token,
        },
    );
    run(listener, app, service, engine).await?;
    Ok(ExitCode::SUCCESS)
}

/// Serves the API and runs the queue until Ctrl-C.
async fn run(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    service: fangst_downloads::Service,
    engine: Option<fangst_torrent::Engine>,
) -> anyhow::Result<()> {
    let worker = tokio::spawn(async move { service.run().await });
    // The client address feeds the login rate limit.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    worker.abort();
    if let Some(engine) = engine {
        engine.shutdown().await;
    }
    Ok(())
}

/// The API key from the environment, if one is set. Without one the owner gets a generated key,
/// shown on the connection details page.
fn api_key(args: &ServeArgs) -> anyhow::Result<Option<String>> {
    let Some(api_key) = std::env::var(&args.api_key_env)
        .ok()
        .filter(|k| !k.is_empty())
    else {
        return Ok(None);
    };
    if api_key.len() < 16 {
        bail!(
            "the API key in {} must be at least 16 characters",
            args.api_key_env
        );
    }
    Ok(Some(api_key))
}

/// Variable holding the owner's password, so a container needs no setup in the web UI.
const OWNER_PASSWORD_ENV: &str = "FANGST_OWNER_PASSWORD";

/// 96 bits: far beyond online guessing under the login rate limit, and short enough to copy.
const SETUP_TOKEN_CHARS: usize = 24;

struct Owner {
    /// The password came from the environment, which counts as a finished setup.
    from_env: bool,
    /// Claims the owner account in the web UI while it has no password.
    setup_token: Option<String>,
}

/// Creates the owner on first start, with the environment's API key and password when set.
async fn accounts(
    auth: &fangst_downloads::Auth,
    api_key: Option<&str>,
    env: &settings::Env,
) -> anyhow::Result<Owner> {
    let owner = match api_key {
        Some(key) => auth.bootstrap(key).await?,
        None => auth.bootstrap_without_key().await?,
    };
    let password = env(OWNER_PASSWORD_ENV).filter(|p| !p.is_empty());
    if let Some(password) = &password
        && !auth.check_password(owner.id, password).await?
    {
        // Only on a change: setting a password ends the owner's sessions.
        auth.set_password(owner.id, password)
            .await
            .with_context(|| format!("{OWNER_PASSWORD_ENV} is not usable"))?;
    }
    let setup_token = if auth.has_password(owner.id).await? {
        None
    } else {
        Some(
            fangst_core::secret::random_token()?
                .chars()
                .take(SETUP_TOKEN_CHARS)
                .collect(),
        )
    };
    Ok(Owner {
        from_env: password.is_some(),
        setup_token,
    })
}

/// The data folder, with `fangst.toml` written on first start, and the settings layered over it.
fn load_settings(
    args: &ServeArgs,
    env: &settings::Env,
) -> anyhow::Result<(PathBuf, Layers, config::Config)> {
    let data_dir = args
        .data_dir
        .clone()
        .or_else(|| env("FANGST_DATA_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("data"));
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating {}", data_dir.display()))?;
    let file_path = data_dir.join(config::FILE_NAME);
    config::ensure_file(&file_path)?;
    let file = config::read_file(&file_path)?.unwrap_or_default();
    let layers = Layers::new(file, config::env_layer(std::env::vars()), args.layer())?;
    let config = layers.resolve().with_context(|| {
        format!(
            "reading the settings in {} and FANGST_* variables",
            file_path.display()
        )
    })?;
    Ok((data_dir, layers, config))
}

struct Providers {
    registry: fangst_providers::Registry,
    /// Adapters and post-processors, enabled or not.
    infos: Vec<fangst_providers::ProviderInfo>,
    /// Every adapter, mounted whether enabled or not so settings can switch them while running.
    adapters: Vec<Arc<dyn fangst_providers::ApiAdapter>>,
}

async fn providers(
    config: &config::Config,
    sources: &settings::Sources,
) -> anyhow::Result<Providers> {
    let enabled = sources.enabled(config);
    let secrets = sources
        .notifier_secrets(config, &HashMap::new())
        .await
        .map_err(startup)?;
    let notifiers = settings::build_notifiers(config, &enabled, &secrets)?;
    let mut available = builtin_providers(&config.defaults);
    let infos = available
        .iter()
        .map(fangst_providers::Provider::info)
        .collect();
    let adapters = available
        .iter()
        .filter_map(|p| match p {
            fangst_providers::Provider::ApiAdapter(a) => Some(Arc::clone(a)),
            _ => None,
        })
        .collect();
    available.extend(
        notifiers
            .into_iter()
            .map(fangst_providers::Provider::Notifier),
    );
    Ok(Providers {
        registry: fangst_providers::Registry::build(available, &enabled)?,
        infos,
        adapters,
    })
}

async fn start_torrents(
    config: &config::Config,
    data_dir: &std::path::Path,
) -> anyhow::Result<Option<fangst_torrent::Engine>> {
    let Some(torrent) = settings::torrent_config(config, data_dir) else {
        return Ok(None);
    };
    let port = torrent.listen.port();
    if let Some(state) = &torrent.state_dir {
        std::fs::create_dir_all(state).with_context(|| format!("creating {}", state.display()))?;
    }
    let engine = fangst_torrent::Engine::start(torrent)
        .await
        .with_context(|| format!("starting the torrent engine on port {port}"))?;
    Ok(Some(engine))
}

/// The safety check runs for every job, whatever `--enable` says: without it, extraction or a
/// plain download could hand an executable to the media library.
const REQUIRED_PROVIDERS: [&str; 1] = ["postproc-safety"];

fn with_required_providers(enabled: &[String]) -> Vec<String> {
    let mut all: Vec<String> = REQUIRED_PROVIDERS
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    all.extend(
        enabled
            .iter()
            .filter(|id| !REQUIRED_PROVIDERS.contains(&id.as_str()))
            .cloned(),
    );
    all
}

/// The adapters and post-processors compiled into this binary; the settings pick from these.
/// Notifiers are built from their settings once enabled, in [`settings::build_notifiers`].
fn builtin_providers(
    defaults: &fangst_core::defaults::Defaults,
) -> Vec<fangst_providers::Provider> {
    let mut providers = vec![
        fangst_providers::Provider::ApiAdapter(Arc::new(fangst_adapter_sabnzbd::Sabnzbd)),
        fangst_providers::Provider::ApiAdapter(Arc::new(fangst_adapter_qbittorrent::Qbittorrent)),
    ];
    providers.extend(post_processors(defaults));
    providers
}

/// Post-processors run in this order once enabled: nothing is extracted before the safety check.
fn post_processors(defaults: &fangst_core::defaults::Defaults) -> [fangst_providers::Provider; 2] {
    use fangst_postproc_extract::{ExtractSettings, Extractor, SafetyCheck, SafetySettings};
    [
        fangst_providers::Provider::PostProcessor(Arc::new(SafetyCheck::new(SafetySettings {
            blocked_extensions: defaults.safety.blocked_extensions.clone(),
        }))),
        fangst_providers::Provider::PostProcessor(Arc::new(Extractor::new(ExtractSettings {
            delete_archives: defaults.extract.delete_archives,
        }))),
    ]
}

/// Notifier settings from the command line, over `[notifiers]` in the config file. Secrets are
/// read from environment variables named here, never taken as values, which would leave them in
/// shell history.
#[derive(clap::Args)]
struct NotifyArgs {
    /// URL that notify-webhook posts each event to as JSON.
    #[arg(long)]
    webhook_url: Option<String>,
    /// Name of the environment variable holding a bearer token for the webhook.
    #[arg(long, requires = "webhook_url")]
    webhook_token_env: Option<String>,
    /// ntfy server that notify-ntfy publishes to. Defaults to `notify.ntfy_server`.
    #[arg(long)]
    ntfy_url: Option<String>,
    /// ntfy topic that notify-ntfy publishes to.
    #[arg(long)]
    ntfy_topic: Option<String>,
    /// Name of the environment variable holding an ntfy access token, for protected topics.
    #[arg(long, requires = "ntfy_topic")]
    ntfy_token_env: Option<String>,
    /// Name of the environment variable holding the Discord webhook URL for notify-discord; the
    /// URL itself contains the token that lets anyone post to the channel.
    #[arg(long)]
    discord_webhook_env: Option<String>,
}

impl NotifyArgs {
    fn layer(&self, layer: &mut Table) {
        for (path, value) in [
            (["webhook", "url"], &self.webhook_url),
            (["webhook", "token_env"], &self.webhook_token_env),
            (["ntfy", "server"], &self.ntfy_url),
            (["ntfy", "topic"], &self.ntfy_topic),
            (["ntfy", "token_env"], &self.ntfy_token_env),
            (["discord", "webhook_url_env"], &self.discord_webhook_env),
        ] {
            if let Some(value) = value {
                set(
                    layer,
                    &["notifiers", path[0], path[1]],
                    value.clone().into(),
                );
            }
        }
    }
}

/// The start-up banner. Tests and the browser smoke test read the first line for the address.
fn announce(
    addr: std::net::SocketAddr,
    has_server: bool,
    engine: Option<&fangst_torrent::Engine>,
    registry: &fangst_providers::Registry,
) {
    println!("listening on {addr}");
    if !has_server {
        println!("no usenet server configured; usenet jobs will wait in the queue");
    }
    println!("{}", describe_torrents(engine));
    println!("{}", describe_providers(registry));
}

fn describe_torrents(engine: Option<&fangst_torrent::Engine>) -> String {
    match engine.map(fangst_torrent::Engine::listen_addr) {
        Some(Some(addr)) => format!("torrents: listening on {addr}"),
        Some(None) => "torrents: on, not listening for peers".to_owned(),
        None => "torrents: off; torrent jobs will wait in the queue".to_owned(),
    }
}

fn describe_providers(registry: &fangst_providers::Registry) -> String {
    let enabled: Vec<&str> = registry
        .listing()
        .iter()
        .filter(|(_, on)| *on)
        .map(|(info, _)| info.id.as_str())
        .collect();
    if enabled.is_empty() {
        format!(
            "providers: none enabled ({} available)",
            registry.listing().len()
        )
    } else {
        format!("providers: {}", enabled.join(", "))
    }
}

async fn reset_password(args: ResetPasswordArgs) -> anyhow::Result<ExitCode> {
    let db = args.data_dir.join("downloads.db");
    let key = args.data_dir.join("master.key");
    // Creating either file here would start an empty install, or a new master key that can't
    // read the stored secrets, in what is most likely a mistyped folder.
    if !db.exists() || !key.exists() {
        bail!(
            "{} has no downloads.db and master.key; point --data-dir at the daemon's data folder",
            args.data_dir.display()
        );
    }
    let password = if let Some(var) = &args.password_env {
        std::env::var(var).with_context(|| format!("environment variable {var} is not set"))?
    } else {
        use std::io::{BufRead, IsTerminal};
        if std::io::stdin().is_terminal() {
            eprint!("new password for {} (shown as you type): ", args.user);
        }
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .context("reading the password from standard input")?;
        line.trim_end_matches(['\r', '\n']).to_owned()
    };
    let store = fangst_downloads::Store::open(&db).await?;
    let master_key = fangst_core::secret::MasterKey::load_or_create(&key)?;
    let auth = fangst_downloads::Auth::new(&store, Arc::new(master_key)).await?;
    let Some(user) = auth.user_by_name(&args.user).await? else {
        bail!("there is no user named {}", args.user);
    };
    auth.set_password(user.id, &password).await?;
    println!(
        "password for {} set; their sessions have ended",
        user.username
    );
    Ok(ExitCode::SUCCESS)
}

fn openapi(args: &OpenapiArgs) -> anyhow::Result<ExitCode> {
    let generated = fangst_dl_api::openapi_json();
    if !args.check {
        std::fs::write(&args.path, &generated)
            .with_context(|| format!("writing {}", args.path.display()))?;
        return Ok(ExitCode::SUCCESS);
    }
    let committed = std::fs::read_to_string(&args.path).unwrap_or_default();
    // Git may check the file out with CRLF on Windows; only the content matters.
    if committed.replace("\r\n", "\n") == generated {
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!(
            "{} is out of date with the API code. Run `just openapi` and commit the result.",
            args.path.display()
        );
        Ok(ExitCode::FAILURE)
    }
}

async fn get(args: GetArgs) -> anyhow::Result<ExitCode> {
    let bytes =
        std::fs::read(&args.nzb).with_context(|| format!("reading {}", args.nzb.display()))?;
    let nzb = fangst_nzb::parse(&bytes, &fangst_nzb::Limits::default())
        .with_context(|| format!("parsing {}", args.nzb.display()))?;
    let Some(server) = args.usenet.server()? else {
        bail!("--host is required")
    };
    let mut layer = Table::new();
    args.usenet.layer(&mut layer);
    let defaults = Layers::new(Table::new(), Table::new(), layer)?
        .resolve()?
        .defaults
        .usenet;
    let tls = if server.tls.unwrap_or(defaults.tls) {
        Some(fangst_nntp::native_tls_config()?)
    } else {
        None
    };
    let max_connections = server.connections.unwrap_or(defaults.connections);
    let pool = Pool::new(ServerConfig {
        host: server.host,
        port: server.port.unwrap_or(defaults.port),
        tls,
        credentials: server
            .username
            .map(|u| (u, server.password.unwrap_or_default())),
        max_connections,
        connect_timeout: defaults.connect_timeout(),
        command_timeout: defaults.command_timeout(),
    });
    let options = fangst_usenet::Options {
        connections: max_connections,
        pipeline_depth: defaults.pipeline,
        retries: defaults.retries,
    };
    let progress: fangst_usenet::ProgressFn = Arc::new(|p: &Progress| {
        if p.file_finished() {
            println!("done  {}  ({} bytes)", p.file_name, p.bytes_written);
        }
    });

    println!(
        "downloading {} files ({} bytes) to {}",
        nzb.files.len(),
        nzb.total_bytes(),
        args.out.display()
    );
    let report =
        fangst_usenet::download(&nzb, &args.out, Arc::new(pool), options, progress).await?;

    for file in report.files.iter().filter(|f| !f.is_complete()) {
        if !file.missing_segments.is_empty() {
            println!(
                "missing  {}  segments {:?}",
                file.name, file.missing_segments
            );
        }
        for (segment, reason) in &file.failed_segments {
            println!("failed  {}  segment {segment}: {reason}", file.name);
        }
    }
    if report.is_complete() {
        println!("complete");
        Ok(ExitCode::SUCCESS)
    } else {
        println!("incomplete: repair is not available yet");
        Ok(ExitCode::from(2))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use clap::Parser;
    use fangst_core::config::{Layers, Table};

    use super::{Cli, Command, ServeArgs, builtin_providers, with_required_providers};
    use crate::settings;

    fn serve_args(flags: &[&str]) -> ServeArgs {
        let args = ["fangst-dl", "serve"].iter().chain(flags);
        match Cli::try_parse_from(args).unwrap().command {
            Command::Serve(args) => *args,
            _ => unreachable!(),
        }
    }

    /// The providers `serve` would build from these flags and environment variables.
    async fn provider_ids(flags: &[&str], env: &[(&str, &str)]) -> anyhow::Result<Vec<String>> {
        let args = serve_args(flags);
        let env: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let config = Layers::new(Table::new(), Table::new(), args.layer())?.resolve()?;
        let dir = tempfile::tempdir()?;
        let store = fangst_downloads::Store::open(&dir.path().join("downloads.db")).await?;
        let key = fangst_core::secret::MasterKey::from_bytes(&[3; 32]);
        let sources = settings::Sources {
            secrets: fangst_downloads::Secrets::new(&store, Arc::new(key)),
            env: Arc::new(move |name| env.get(name).cloned()),
            extra_server: None,
            required: with_required_providers(&args.enabled_providers),
        };
        let secrets = sources
            .notifier_secrets(&config, &HashMap::new())
            .await
            .map_err(super::startup)?;
        let notifiers = settings::build_notifiers(&config, &sources.enabled(&config), &secrets)?;
        let mut ids: Vec<String> = builtin_providers(&config.defaults)
            .iter()
            .map(|p| p.info().id)
            .collect();
        ids.extend(notifiers.iter().map(|n| n.info().id));
        Ok(ids)
    }

    const ALL_SETTINGS: &[&str] = &[
        "--webhook-url",
        "https://example.com/hook",
        "--webhook-token-env",
        "HOOK_TOKEN",
        "--ntfy-topic",
        "fangst",
        "--ntfy-token-env",
        "NTFY_TOKEN",
        "--discord-webhook-env",
        "DISCORD_URL",
    ];
    const ALL_ENV: &[(&str, &str)] = &[
        ("HOOK_TOKEN", "t1"),
        ("NTFY_TOKEN", "tk_2"),
        ("DISCORD_URL", "https://discord.com/api/webhooks/1/secret"),
    ];

    #[tokio::test]
    async fn notifier_settings_alone_build_no_notifier() {
        // Adapters and post-processors need no settings, so they're always compiled in; notifiers
        // only when enabled.
        assert_eq!(
            provider_ids(ALL_SETTINGS, ALL_ENV).await.unwrap(),
            [
                "adapter-sabnzbd",
                "adapter-qbittorrent",
                "postproc-safety",
                "postproc-extract"
            ]
        );
    }

    #[tokio::test]
    async fn each_enabled_notifier_is_built_from_its_settings() {
        let mut flags = ALL_SETTINGS.to_vec();
        flags.extend([
            "--enable",
            "notify-discord",
            "--enable",
            "notify-ntfy",
            "--enable",
            "notify-webhook",
        ]);
        assert_eq!(
            provider_ids(&flags, ALL_ENV).await.unwrap(),
            [
                "adapter-sabnzbd",
                "adapter-qbittorrent",
                "postproc-safety",
                "postproc-extract",
                "notify-webhook",
                "notify-ntfy",
                "notify-discord"
            ]
        );

        let ntfy_only =
            provider_ids(&["--ntfy-topic", "fangst", "--enable", "notify-ntfy"], &[]).await;
        assert_eq!(
            ntfy_only.unwrap(),
            [
                "adapter-sabnzbd",
                "adapter-qbittorrent",
                "postproc-safety",
                "postproc-extract",
                "notify-ntfy"
            ]
        );
    }

    #[test]
    fn safety_check_runs_before_extraction_whatever_the_enable_order() {
        let args = serve_args(&[
            "--enable",
            "postproc-extract",
            "--enable",
            "postproc-safety",
        ]);
        let available = builtin_providers(&fangst_core::defaults::Defaults::builtin().unwrap());
        let registry =
            fangst_providers::Registry::build(available, &args.enabled_providers).unwrap();
        let order: Vec<String> = registry
            .post_processors
            .iter()
            .map(|p| p.info().id)
            .collect();
        assert_eq!(order, ["postproc-safety", "postproc-extract"]);
    }

    #[tokio::test]
    async fn enabled_notifier_without_settings_names_the_missing_flag() {
        for (id, flag) in [
            ("notify-webhook", "--webhook-url"),
            ("notify-ntfy", "--ntfy-topic"),
            ("notify-discord", "--discord-webhook-env"),
        ] {
            let err = provider_ids(&["--enable", id], &[]).await.unwrap_err();
            assert!(err.to_string().contains(flag), "{id}: {err}");
        }
    }

    #[tokio::test]
    async fn missing_environment_variable_is_named() {
        let err = provider_ids(
            &[
                "--discord-webhook-env",
                "DISCORD_URL",
                "--enable",
                "notify-discord",
            ],
            &[],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("DISCORD_URL"), "{err}");
    }

    #[tokio::test]
    async fn invalid_settings_stop_startup() {
        let err = provider_ids(
            &["--ntfy-topic", "no spaces", "--enable", "notify-ntfy"],
            &[],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("topic"), "{err}");
    }

    #[test]
    fn token_flags_need_their_setting() {
        let args = ["fangst-dl", "serve", "--ntfy-token-env", "NTFY_TOKEN"];
        assert!(Cli::try_parse_from(args).is_err());
    }

    #[test]
    fn flags_become_the_top_settings_layer() {
        let args = serve_args(&[
            "--no-torrent",
            "--torrent-port",
            "7000",
            "--listen",
            "0.0.0.0:1",
            "--pipeline",
            "3",
            "--ntfy-topic",
            "t",
        ]);
        let mut file = Table::new();
        fangst_core::config::insert_path(
            &mut file,
            &["torrent".to_owned(), "listen_port".to_owned()],
            6000.into(),
        );
        let layers = Layers::new(file, Table::new(), args.layer()).unwrap();
        let config = layers.resolve().unwrap();
        assert!(!config.defaults.torrent.enabled);
        assert_eq!(config.defaults.torrent.listen_port, 7000);
        assert_eq!(config.defaults.http.listen, "0.0.0.0:1");
        assert_eq!(config.defaults.usenet.pipeline, 3);
        assert_eq!(config.settings.notifiers.ntfy.topic.as_deref(), Some("t"));
        assert!(layers.locked().contains("torrent.listen_port"));
    }
}

#[cfg(test)]
mod required_provider_tests {
    use super::with_required_providers;

    #[test]
    fn safety_is_always_enabled_once() {
        assert_eq!(with_required_providers(&[]), ["postproc-safety"]);
        assert_eq!(
            with_required_providers(&["postproc-extract".to_owned(), "postproc-safety".to_owned()]),
            ["postproc-safety", "postproc-extract"]
        );
    }
}
