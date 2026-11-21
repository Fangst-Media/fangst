//! Measures the usenet engine against the in-process mock NNTP server and compares the result with
//! this machine's saved baseline, so a change that slows downloads down shows up as a failure.
//!
//! The mock server runs in the same process, so CPU numbers include the server's encoding and
//! sending work. They are useful for comparing runs on one machine, not as absolute figures.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;
use cpu_time::ProcessTime;
use fangst_bench::{Config, Library, MockServer};
use fangst_nntp::{Pool, ServerConfig};
use fangst_usenet::{Options, Resume, Server};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(
    name = "usenet-bench",
    about = "Benchmark the usenet engine against the mock NNTP server"
)]
struct Args {
    /// Size of the downloaded file in MiB.
    #[arg(long, default_value_t = 256)]
    size_mib: usize,
    /// Article size in KiB; 750 KiB is typical for real posts.
    #[arg(long, default_value_t = 750)]
    segment_kib: usize,
    #[arg(long, default_value_t = 8)]
    connections: usize,
    #[arg(long, default_value_t = 16)]
    pipeline: usize,
    /// Per-command latency of the mock server, to mimic a remote provider.
    #[arg(long, default_value_t = 0)]
    latency_ms: u64,
    /// Store this run as the machine's baseline instead of comparing against it.
    #[arg(long)]
    save_baseline: bool,
    /// Allowed slowdown against the baseline before the run fails (0.15 = 15%).
    #[arg(long, default_value_t = 0.15)]
    tolerance: f64,
    /// Folder holding per-machine baselines.
    #[arg(long, default_value = "bench")]
    baseline_dir: PathBuf,
    /// Also write the result as JSON here, for CI artifacts.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Result_ {
    machine: String,
    scenario: String,
    /// Usenet download throughput, decoded bytes.
    mbit_per_sec: f64,
    /// CPU seconds the whole process spent per gigabit downloaded.
    cpu_sec_per_gbit: f64,
    /// Single-core yEnc decode speed.
    yenc_decode_gb_per_sec: f64,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> anyhow::Result<ExitCode> {
    let runtime = tokio::runtime::Runtime::new()?;
    let download = runtime.block_on(download(args))?;
    let result = Result_ {
        machine: machine_name(),
        scenario: scenario(args),
        mbit_per_sec: download.0,
        cpu_sec_per_gbit: download.1,
        yenc_decode_gb_per_sec: yenc_decode_speed(),
    };
    println!(
        "{}: {:.0} Mbit/s, {:.2} CPU s per Gbit, yEnc decode {:.2} GB/s per core",
        result.scenario,
        result.mbit_per_sec,
        result.cpu_sec_per_gbit,
        result.yenc_decode_gb_per_sec
    );
    if let Some(out) = &args.out {
        std::fs::write(out, serde_json::to_string_pretty(&result)?)
            .with_context(|| format!("writing {}", out.display()))?;
    }

    let path = baseline_path(&args.baseline_dir, &result.machine, &result.scenario);
    if args.save_baseline {
        std::fs::create_dir_all(&args.baseline_dir)?;
        std::fs::write(&path, serde_json::to_string_pretty(&result)?)?;
        println!("saved baseline {}", path.display());
        return Ok(ExitCode::SUCCESS);
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!("no baseline for this machine and scenario yet; run with --save-baseline");
        return Ok(ExitCode::SUCCESS);
    };
    let baseline: Result_ =
        serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
    let regressions = compare(&baseline, &result, args.tolerance);
    for line in &regressions {
        println!("regression: {line}");
    }
    Ok(if regressions.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn scenario(args: &Args) -> String {
    format!(
        "{}MiB-{}KiB-c{}-d{}-l{}ms",
        args.size_mib, args.segment_kib, args.connections, args.pipeline, args.latency_ms
    )
}

fn machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map_or_else(|| "unknown".to_owned(), |name| name.trim().to_owned())
}

fn baseline_path(dir: &Path, machine: &str, scenario: &str) -> PathBuf {
    let safe: String = machine
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{safe}.{scenario}.json"))
}

/// Lines describing every measure that got worse than `tolerance` allows.
fn compare(baseline: &Result_, now: &Result_, tolerance: f64) -> Vec<String> {
    let mut out = Vec::new();
    let mut lower_is_worse = |name: &str, base: f64, value: f64| {
        if value < base * (1.0 - tolerance) {
            out.push(format!(
                "{name} {value:.2} is more than {:.0}% below the baseline {base:.2}",
                tolerance * 100.0
            ));
        }
    };
    lower_is_worse("throughput Mbit/s", baseline.mbit_per_sec, now.mbit_per_sec);
    lower_is_worse(
        "yEnc decode GB/s",
        baseline.yenc_decode_gb_per_sec,
        now.yenc_decode_gb_per_sec,
    );
    if now.cpu_sec_per_gbit > baseline.cpu_sec_per_gbit * (1.0 + tolerance) {
        out.push(format!(
            "CPU s per Gbit {:.2} is more than {:.0}% above the baseline {:.2}",
            now.cpu_sec_per_gbit,
            tolerance * 100.0,
            baseline.cpu_sec_per_gbit
        ));
    }
    out
}

fn test_data(len: usize) -> Vec<u8> {
    // xorshift: cheap, deterministic, and incompressible enough to look like real media.
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

async fn download(args: &Args) -> anyhow::Result<(f64, f64)> {
    let data = test_data(args.size_mib * 1024 * 1024);
    let mut library = Library::new();
    library.add_file(
        "bench.bin",
        &data,
        args.segment_kib * 1024,
        "alt.binaries.bench",
    );
    let nzb = fangst_nzb::parse(library.nzb().as_bytes(), &fangst_nzb::Limits::default())?;
    let config = Config {
        latency: Duration::from_millis(args.latency_ms),
        max_connections: args.connections,
        ..Config::default()
    };
    let server = MockServer::start(library, config).await?;
    let pool = Arc::new(Pool::new(ServerConfig {
        host: "127.0.0.1".to_owned(),
        port: server.addr.port(),
        tls: None,
        credentials: None,
        max_connections: args.connections,
        connect_timeout: Duration::from_secs(10),
        command_timeout: Duration::from_mins(1),
    }));
    let out = tempfile::tempdir()?;
    let options = Options {
        connections: args.connections,
        pipeline_depth: args.pipeline,
        retries: 1,
    };

    let cpu = ProcessTime::now();
    let started = Instant::now();
    let report = fangst_usenet::download_from(
        &nzb,
        out.path(),
        vec![Server::primary(pool)],
        options,
        Arc::new(|_| {}),
        Resume::default(),
    )
    .await?;
    let wall = started.elapsed().as_secs_f64();
    let cpu = cpu.elapsed().as_secs_f64();
    anyhow::ensure!(
        report.is_complete(),
        "the benchmark download did not complete: {report:?}"
    );

    #[allow(clippy::cast_precision_loss)]
    let gbit = (data.len() as f64) * 8.0 / 1e9;
    Ok((gbit * 1000.0 / wall, cpu / gbit))
}

fn yenc_decode_speed() -> f64 {
    let data = test_data(64 * 1024 * 1024);
    let article = fangst_yenc::encode_single(&data, "bench.bin", 128);
    let started = Instant::now();
    let mut decoded = 0usize;
    // Decode for at least a second so timer resolution doesn't matter.
    while started.elapsed() < Duration::from_secs(1) {
        decoded += fangst_yenc::decode(&article).map_or(0, |d| d.data.len());
    }
    #[allow(clippy::cast_precision_loss)]
    let gb = decoded as f64 / 1e9;
    gb / started.elapsed().as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(mbit: f64, cpu: f64, yenc: f64) -> Result_ {
        Result_ {
            machine: "m".to_owned(),
            scenario: "s".to_owned(),
            mbit_per_sec: mbit,
            cpu_sec_per_gbit: cpu,
            yenc_decode_gb_per_sec: yenc,
        }
    }

    #[test]
    fn within_tolerance_passes() {
        assert_eq!(
            compare(&result(1000.0, 1.0, 2.0), &result(860.0, 1.14, 1.71), 0.15),
            [] as [std::string::String; 0]
        );
    }

    #[test]
    fn each_measure_can_regress() {
        let base = result(1000.0, 1.0, 2.0);
        assert_eq!(compare(&base, &result(840.0, 1.0, 2.0), 0.15).len(), 1);
        assert_eq!(compare(&base, &result(1000.0, 1.2, 2.0), 0.15).len(), 1);
        assert_eq!(compare(&base, &result(1000.0, 1.0, 1.6), 0.15).len(), 1);
        assert_eq!(compare(&base, &result(500.0, 3.0, 0.5), 0.15).len(), 3);
    }

    #[test]
    fn getting_faster_is_never_a_regression() {
        assert_eq!(
            compare(&result(1000.0, 1.0, 2.0), &result(5000.0, 0.1, 9.0), 0.15),
            [] as [std::string::String; 0]
        );
    }

    #[test]
    fn baseline_file_names_are_safe() {
        let path = baseline_path(Path::new("bench"), "My PC/../x", "256MiB");
        assert_eq!(path, Path::new("bench").join("My_PC____x.256MiB.json"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_small_run_completes() {
        let args = Args::parse_from([
            "usenet-bench",
            "--size-mib",
            "4",
            "--segment-kib",
            "256",
            "--connections",
            "2",
        ]);
        let (mbit, cpu) = download(&args).await.unwrap();
        assert!(mbit > 0.0 && cpu >= 0.0);
    }
}
