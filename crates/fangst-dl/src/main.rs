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
    Serve(ServeArgs),
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
        Command::Serve(args) => runtime.block_on(serve(args)),
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
    let registry = fangst_providers::Registry::build(builtin_providers(), &args.enabled_providers)?;
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
    };
    let service = fangst_downloads::Service::new(runner);

    let listener = tokio::net::TcpListener::bind(&args.listen)
        .await
        .with_context(|| format!("listening on {}", args.listen))?;
    println!("listening on {}", listener.local_addr()?);
    if !has_server {
        println!("no usenet server configured; jobs will wait in the queue");
    }
    println!("{}", describe_providers(&registry));

    let settings = fangst_dl_api::AuthSettings {
        secure_cookie: args.secure_cookie,
        ..fangst_dl_api::AuthSettings::default()
    };
    let app = fangst_dl_api::router(service.clone(), auth, settings);
    let worker = has_server.then(|| {
        let service = service.clone();
        tokio::spawn(async move { service.run().await })
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
    if let Some(worker) = worker {
        worker.abort();
    }
    Ok(ExitCode::SUCCESS)
}

/// Every provider compiled into this binary; `--enable` picks from these.
fn builtin_providers() -> Vec<fangst_providers::Provider> {
    Vec::new()
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
