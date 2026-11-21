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
    /// Download the files of one NZB from one usenet server, then exit.
    Get(GetArgs),
}

#[derive(clap::Args)]
struct GetArgs {
    /// The NZB file to download.
    nzb: PathBuf,
    /// Folder the files are written to; created if missing.
    #[arg(long, short)]
    out: PathBuf,
    #[arg(long)]
    host: String,
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
        Command::Get(args) => runtime.block_on(get(args)),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn get(args: GetArgs) -> anyhow::Result<ExitCode> {
    let bytes =
        std::fs::read(&args.nzb).with_context(|| format!("reading {}", args.nzb.display()))?;
    let nzb = fangst_nzb::parse(&bytes, &fangst_nzb::Limits::default())
        .with_context(|| format!("parsing {}", args.nzb.display()))?;

    let credentials = match (&args.user, &args.password_env) {
        (Some(user), Some(var)) => {
            let password = std::env::var(var)
                .with_context(|| format!("environment variable {var} is not set"))?;
            Some((user.clone(), password))
        }
        (Some(_), None) => bail!("--user needs --password-env"),
        _ => None,
    };
    let tls = if args.no_tls {
        None
    } else {
        Some(fangst_nntp::native_tls_config()?)
    };
    let pool = Arc::new(Pool::new(ServerConfig {
        host: args.host,
        port: args.port,
        tls,
        credentials,
        max_connections: args.connections,
        connect_timeout: Duration::from_secs(args.connect_timeout),
        command_timeout: Duration::from_secs(args.command_timeout),
    }));
    let options = Options {
        connections: args.connections,
        pipeline_depth: args.pipeline,
        retries: 3,
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
    let report = fangst_usenet::download(&nzb, &args.out, pool, options, progress).await?;

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
