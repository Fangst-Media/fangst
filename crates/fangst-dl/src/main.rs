use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use fangst_nntp::{Pool, ServerConfig};
use fangst_usenet::{Options, Progress};

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

/// One usenet server. Config files and several servers arrive with #16 and #31.
#[derive(clap::Args)]
struct UsenetArgs {
    #[arg(long)]
    host: Option<String>,
    #[arg(long, default_value_t = 563)]
    port: u16,
    /// Connect without TLS (usually port 119).
    #[arg(long)]
    no_tls: bool,
    #[arg(long)]
    user: Option<String>,
    /// Name of the environment variable holding the password, so it stays out of shell history.
    #[arg(long, requires = "user")]
    password_env: Option<String>,
    #[arg(long, default_value_t = 8)]
    connections: usize,
    #[arg(long, default_value_t = 8)]
    pipeline: usize,
    /// Seconds to wait for DNS, TCP and TLS when opening a connection.
    #[arg(long, default_value_t = 15)]
    connect_timeout: u64,
    /// Seconds to wait for any single reply; large articles on slow links need a generous value.
    #[arg(long, default_value_t = 60)]
    command_timeout: u64,
}

impl UsenetArgs {
    fn pool(&self) -> anyhow::Result<Option<Pool>> {
        let Some(host) = &self.host else {
            return Ok(None);
        };
        let credentials = match (&self.user, &self.password_env) {
            (Some(user), Some(var)) => {
                let password = std::env::var(var)
                    .with_context(|| format!("environment variable {var} is not set"))?;
                Some((user.clone(), password))
            }
            (Some(_), None) => bail!("--user needs --password-env"),
            _ => None,
        };
        let tls = if self.no_tls {
            None
        } else {
            Some(fangst_nntp::native_tls_config()?)
        };
        Ok(Some(Pool::new(ServerConfig {
            host: host.clone(),
            port: self.port,
            tls,
            credentials,
            max_connections: self.connections,
            connect_timeout: Duration::from_secs(self.connect_timeout),
            command_timeout: Duration::from_secs(self.command_timeout),
        })))
    }

    fn options(&self) -> Options {
        Options {
            connections: self.connections,
            pipeline_depth: self.pipeline,
            retries: 3,
        }
    }
}

#[derive(clap::Args)]
struct ServeArgs {
    /// Folder for downloads.db and the downloads themselves.
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,
    /// Address the API listens on. Port 0 picks a free port.
    #[arg(long, default_value = "127.0.0.1:7740")]
    listen: String,
    /// Name of the environment variable holding the API key. It becomes the first user's key and
    /// replaces the one set at the previous start.
    #[arg(long, default_value = "FANGST_API_KEY")]
    api_key_env: String,
    /// Turn on an optional provider (post-processor, notifier, API adapter) by its id. Repeatable.
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

/// The torrent engine. On unless `--no-torrent`; its state lives in `<data-dir>/torrent`.
#[derive(clap::Args)]
#[allow(clippy::struct_excessive_bools)] // Each is an independent command-line switch.
struct TorrentArgs {
    /// Run without the torrent engine; torrent jobs can still be added and wait in the queue.
    #[arg(long)]
    no_torrent: bool,
    /// TCP port peers connect to; 0 picks a free one at every start. Defaults to
    /// `torrent.listen_port` in the defaults file.
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
    /// Download limit across all torrents, in bytes per second. Unlimited when left out.
    #[arg(long, value_name = "BYTES_PER_SEC")]
    torrent_download_limit: Option<NonZeroU32>,
    /// Upload limit across all torrents, in bytes per second. Unlimited when left out.
    #[arg(long, value_name = "BYTES_PER_SEC")]
    torrent_upload_limit: Option<NonZeroU32>,
    /// Torrent jobs downloading at once. Defaults to `torrent.max_active` in the defaults file.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
    max_active_torrents: Option<u32>,
}

impl TorrentArgs {
    async fn engine(
        &self,
        data_dir: &std::path::Path,
        defaults: &fangst_core::defaults::Torrent,
    ) -> anyhow::Result<Option<fangst_torrent::Engine>> {
        if self.no_torrent {
            return Ok(None);
        }
        let state_dir = data_dir.join("torrent");
        std::fs::create_dir_all(&state_dir)
            .with_context(|| format!("creating {}", state_dir.display()))?;
        let port = self.torrent_port.unwrap_or(defaults.listen_port);
        let engine = fangst_torrent::Engine::start(fangst_torrent::Config {
            listen: (std::net::Ipv4Addr::UNSPECIFIED, port).into(),
            announce_port: self.torrent_announce_port,
            dht: !self.no_dht,
            local_discovery: !self.no_local_discovery,
            upnp: !self.no_upnp,
            state_dir: Some(state_dir),
            download_limit: self.torrent_download_limit,
            upload_limit: self.torrent_upload_limit,
        })
        .await
        .with_context(|| format!("starting the torrent engine on port {port}"))?;
        Ok(Some(engine))
    }

    fn max_active(&self, defaults: &fangst_core::defaults::Torrent) -> usize {
        self.max_active_torrents
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(defaults.max_active)
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

async fn serve(args: ServeArgs) -> anyhow::Result<ExitCode> {
    let api_key = std::env::var(&args.api_key_env).with_context(|| {
        format!(
            "set the API key in the {} environment variable",
            args.api_key_env
        )
    })?;
    if api_key.len() < 16 {
        bail!(
            "the API key in {} must be at least 16 characters",
            args.api_key_env
        );
    }
    let defaults = fangst_core::defaults::Defaults::builtin()?;
    let enabled = with_required_providers(&args.enabled_providers);
    let available = builtin_providers(&args.notify, &enabled, &defaults, |name| {
        std::env::var(name).ok()
    })?;
    let registry = fangst_providers::Registry::build(available, &enabled)?;
    std::fs::create_dir_all(&args.data_dir)
        .with_context(|| format!("creating {}", args.data_dir.display()))?;
    let store = fangst_downloads::Store::open(&args.data_dir.join("downloads.db")).await?;
    let master_key =
        fangst_core::secret::MasterKey::load_or_create(&args.data_dir.join("master.key"))?;
    let auth = fangst_downloads::Auth::new(&store, Arc::new(master_key)).await?;
    auth.bootstrap(&api_key).await?;
    let pool = args.usenet.pool()?;
    let has_server = pool.is_some();
    let runner = fangst_downloads::Runner {
        store,
        // Without a server the queue still accepts jobs; they wait until one is configured.
        servers: vec![fangst_usenet::Server::primary(Arc::new(
            pool.unwrap_or_else(|| Pool::new(placeholder_server())),
        ))],
        root: args.data_dir.join("downloads"),
        options: args.usenet.options(),
        post_processors: registry.post_processors.clone(),
    };
    let mut service = fangst_downloads::Service::new(runner)
        .with_notifications(fangst_downloads::Notifications {
            notifiers: registry.notifiers.clone(),
            timeout: defaults.notify.timeout(),
            min_free_bytes: defaults.disk.min_free_bytes,
            disk_check_interval: defaults.disk.check_interval(),
        })
        .with_progress(defaults.progress.clone())
        .with_seeding(defaults.seeding.clone())
        .with_torrent_limit(args.torrent.max_active(&defaults.torrent));
    if !has_server {
        service = service.without_usenet();
    }
    let engine = args
        .torrent
        .engine(&args.data_dir, &defaults.torrent)
        .await?;
    if let Some(engine) = &engine {
        service = service.with_torrents(engine.clone());
    }

    let listener = tokio::net::TcpListener::bind(&args.listen)
        .await
        .with_context(|| format!("listening on {}", args.listen))?;
    println!("listening on {}", listener.local_addr()?);
    if !has_server {
        println!("no usenet server configured; usenet jobs will wait in the queue");
    }
    println!("{}", describe_torrents(engine.as_ref()));
    println!("{}", describe_providers(&registry));

    let settings = fangst_dl_api::AuthSettings {
        secure_cookie: args.secure_cookie,
        ..fangst_dl_api::AuthSettings::default()
    };
    let app = fangst_dl_api::router_with_adapters(
        service.clone(),
        auth,
        settings,
        fangst_dl_api::FetchLimits::default(),
        &registry.adapters,
    );
    let worker = tokio::spawn({
        let service = service.clone();
        async move { service.run().await }
    });
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
    Ok(ExitCode::SUCCESS)
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

/// Every provider compiled into this binary; `--enable` picks from these. Notifiers are only built
/// when enabled, since they need their settings.
fn builtin_providers(
    notify: &NotifyArgs,
    enabled: &[String],
    defaults: &fangst_core::defaults::Defaults,
    env: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<Vec<fangst_providers::Provider>> {
    let mut providers = vec![
        fangst_providers::Provider::ApiAdapter(Arc::new(fangst_adapter_sabnzbd::Sabnzbd)),
        fangst_providers::Provider::ApiAdapter(Arc::new(fangst_adapter_qbittorrent::Qbittorrent)),
    ];
    providers.extend(post_processors(defaults));
    providers.extend(notify.providers(enabled, defaults.notify.timeout(), env)?);
    Ok(providers)
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

/// Notifier settings until the config file arrives (#31). Secrets are read from environment
/// variables named here, never taken as values, which would leave them in shell history.
#[derive(clap::Args)]
struct NotifyArgs {
    /// URL that notify-webhook posts each event to as JSON.
    #[arg(long)]
    webhook_url: Option<String>,
    /// Name of the environment variable holding a bearer token for the webhook.
    #[arg(long, requires = "webhook_url")]
    webhook_token_env: Option<String>,
    /// ntfy server that notify-ntfy publishes to.
    #[arg(long, default_value = "https://ntfy.sh")]
    ntfy_url: String,
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
    fn providers(
        &self,
        enabled: &[String],
        timeout: Duration,
        env: impl Fn(&str) -> Option<String>,
    ) -> anyhow::Result<Vec<fangst_providers::Provider>> {
        let on = |id: &str| enabled.iter().any(|e| e == id);
        let var = |name: &str| {
            env(name).with_context(|| format!("environment variable {name} is not set"))
        };
        let mut notifiers: Vec<Arc<dyn fangst_providers::Notifier>> = Vec::new();
        if on(fangst_notify_webhook::ID) {
            let url = self
                .webhook_url
                .clone()
                .context("notify-webhook needs --webhook-url")?;
            let bearer_token = self.webhook_token_env.as_deref().map(var).transpose()?;
            notifiers.push(Arc::new(fangst_notify_webhook::WebhookNotifier::new(
                fangst_notify_webhook::Settings {
                    url,
                    bearer_token,
                    timeout,
                },
            )?));
        }
        if on(fangst_notify_ntfy::ID) {
            let topic = self
                .ntfy_topic
                .clone()
                .context("notify-ntfy needs --ntfy-topic")?;
            let token = self.ntfy_token_env.as_deref().map(var).transpose()?;
            notifiers.push(Arc::new(fangst_notify_ntfy::NtfyNotifier::new(
                fangst_notify_ntfy::Settings {
                    server: self.ntfy_url.clone(),
                    topic,
                    token,
                    timeout,
                },
            )?));
        }
        if on(fangst_notify_discord::ID) {
            let name = self
                .discord_webhook_env
                .as_deref()
                .context("notify-discord needs --discord-webhook-env")?;
            notifiers.push(Arc::new(fangst_notify_discord::DiscordNotifier::new(
                &fangst_notify_discord::Settings {
                    webhook_url: var(name)?,
                    timeout,
                },
            )?));
        }
        Ok(notifiers
            .into_iter()
            .map(fangst_providers::Provider::Notifier)
            .collect())
    }
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

fn placeholder_server() -> ServerConfig {
    ServerConfig {
        host: "localhost".to_owned(),
        port: 0,
        tls: None,
        credentials: None,
        max_connections: 1,
        connect_timeout: Duration::from_secs(1),
        command_timeout: Duration::from_secs(1),
    }
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
    let Some(pool) = args.usenet.pool()? else {
        bail!("--host is required")
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
    let report = fangst_usenet::download(
        &nzb,
        &args.out,
        Arc::new(pool),
        args.usenet.options(),
        progress,
    )
    .await?;

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

    use clap::Parser;

    use super::{Cli, Command, ServeArgs, builtin_providers};

    fn serve_args(flags: &[&str]) -> ServeArgs {
        let args = ["fangst-dl", "serve"].iter().chain(flags);
        match Cli::try_parse_from(args).unwrap().command {
            Command::Serve(args) => *args,
            _ => unreachable!(),
        }
    }

    fn provider_ids(flags: &[&str], env: &[(&str, &str)]) -> anyhow::Result<Vec<String>> {
        let args = serve_args(flags);
        let env: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let providers = builtin_providers(
            &args.notify,
            &args.enabled_providers,
            &fangst_core::defaults::Defaults::builtin().unwrap(),
            |name| env.get(name).cloned(),
        )?;
        Ok(providers.iter().map(|p| p.info().id).collect())
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

    #[test]
    fn notifier_settings_alone_build_no_notifier() {
        // Adapters and post-processors need no settings, so they're always compiled in; notifiers
        // only when enabled.
        assert_eq!(
            provider_ids(ALL_SETTINGS, ALL_ENV).unwrap(),
            [
                "adapter-sabnzbd",
                "adapter-qbittorrent",
                "postproc-safety",
                "postproc-extract"
            ]
        );
    }

    #[test]
    fn each_enabled_notifier_is_built_from_its_settings() {
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
            provider_ids(&flags, ALL_ENV).unwrap(),
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

        let ntfy_only = provider_ids(&["--ntfy-topic", "fangst", "--enable", "notify-ntfy"], &[]);
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
        let available = builtin_providers(
            &args.notify,
            &args.enabled_providers,
            &fangst_core::defaults::Defaults::builtin().unwrap(),
            |_| None,
        )
        .unwrap();
        let registry =
            fangst_providers::Registry::build(available, &args.enabled_providers).unwrap();
        let order: Vec<String> = registry
            .post_processors
            .iter()
            .map(|p| p.info().id)
            .collect();
        assert_eq!(order, ["postproc-safety", "postproc-extract"]);
    }

    #[test]
    fn enabled_notifier_without_settings_names_the_missing_flag() {
        for (id, flag) in [
            ("notify-webhook", "--webhook-url"),
            ("notify-ntfy", "--ntfy-topic"),
            ("notify-discord", "--discord-webhook-env"),
        ] {
            let err = provider_ids(&["--enable", id], &[]).unwrap_err();
            assert!(err.to_string().contains(flag), "{id}: {err}");
        }
    }

    #[test]
    fn missing_environment_variable_is_named() {
        let err = provider_ids(
            &[
                "--discord-webhook-env",
                "DISCORD_URL",
                "--enable",
                "notify-discord",
            ],
            &[],
        )
        .unwrap_err();
        assert!(err.to_string().contains("DISCORD_URL"), "{err}");
    }

    #[test]
    fn invalid_settings_stop_startup() {
        let err = provider_ids(
            &["--ntfy-topic", "no spaces", "--enable", "notify-ntfy"],
            &[],
        )
        .unwrap_err();
        assert!(err.to_string().contains("topic"), "{err}");
    }

    #[test]
    fn token_flags_need_their_setting() {
        let args = ["fangst-dl", "serve", "--ntfy-token-env", "NTFY_TOKEN"];
        assert!(Cli::try_parse_from(args).is_err());
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
