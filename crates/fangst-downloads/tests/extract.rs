//! Archives posted to the mock usenet server, downloaded by the runner, then checked and extracted
//! by the `postproc-safety` and `postproc-extract` providers in that order.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_downloads::{Runner, Status, Store};
use fangst_nntp::{Pool, ServerConfig};
use fangst_postproc_extract::{ExtractSettings, Extractor, SafetyCheck, SafetySettings};
use fangst_providers::PostProcessor;

const PASSWORD: &str = "fangst-test";

fn payload(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            x.to_be_bytes()[0]
        })
        .collect()
}

fn fixture(name: &str) -> (String, Vec<u8>) {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let path = Path::new(&manifest)
        .join("../../fixtures/archives")
        .join(name);
    let file = path.file_name().unwrap().to_string_lossy().into_owned();
    (file, std::fs::read(path).unwrap())
}

fn post(fixtures: &[&str]) -> Library {
    let mut library = Library::new();
    for name in fixtures {
        let (file, bytes) = fixture(name);
        library.add_file(&file, &bytes, 4000, "alt.binaries.test");
    }
    library
}

/// The NZB with a `<head>` carrying `password` as metadata.
fn with_password(nzb: &str, password: &str) -> String {
    let at = nzb.find('>').unwrap() + 1;
    let at = at + nzb[at..].find('>').unwrap() + 1;
    format!(
        "{}\n  <head><meta type=\"password\">{password}</meta></head>{}",
        &nzb[..at],
        &nzb[at..]
    )
}

fn providers() -> Vec<Arc<dyn PostProcessor>> {
    let defaults = fangst_core::defaults::Defaults::builtin().unwrap();
    vec![
        Arc::new(SafetyCheck::new(SafetySettings {
            blocked_extensions: defaults.safety.blocked_extensions,
        })),
        Arc::new(Extractor::new(ExtractSettings {
            delete_archives: defaults.extract.delete_archives,
        })),
    ]
}

struct Outcome {
    status: Status,
    error: Option<String>,
    folder: PathBuf,
    postprocess: Vec<String>,
}

async fn run(library: Library, name: &str, nzb: String) -> (Outcome, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let id = store
        .add_nzb(name, Some("tv"), 0, nzb.as_bytes())
        .await
        .unwrap();
    let server = MockServer::start(library, Config::default()).await.unwrap();
    let runner = Runner {
        store: store.clone(),
        servers: vec![fangst_usenet::Server::primary(Arc::new(Pool::new(
            ServerConfig {
                host: "127.0.0.1".to_owned(),
                port: server.addr.port(),
                tls: None,
                credentials: None,
                max_connections: 2,
                connect_timeout: Duration::from_secs(5),
                command_timeout: Duration::from_secs(5),
            },
        )))],
        root: dir.path().join("downloads"),
        options: fangst_usenet::Options {
            connections: 2,
            pipeline_depth: 4,
            retries: 1,
        },
        post_processors: providers(),
    };

    runner.run_until_idle().await.unwrap();

    let job = store.get(id).await.unwrap();
    let postprocess = store
        .timeline(id)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "postprocess")
        .map(|e| e.message)
        .collect();
    let outcome = Outcome {
        status: job.status,
        error: job.error,
        folder: job.output_path.unwrap(),
        postprocess,
    };
    (outcome, dir)
}

fn files_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn extract_downloaded_rar_volumes_after_the_safety_check() {
    let library = post(&[
        "rar5-volumes/show.part1.rar",
        "rar5-volumes/show.part2.rar",
        "rar5-volumes/show.part3.rar",
    ]);
    let nzb = library.nzb();

    let (outcome, _dir) = run(library, "show", nzb).await;

    assert_eq!(outcome.status, Status::Completed, "{:?}", outcome.error);
    assert_eq!(files_in(&outcome.folder), ["show.mkv"]);
    assert_eq!(
        std::fs::read(outcome.folder.join("show.mkv")).unwrap(),
        payload(20_000, 1)
    );
    assert_eq!(
        outcome.postprocess,
        [
            "postproc-safety: checked 3 files and 1 archive with 1 entry; nothing blocked",
            "postproc-extract: show.part1.rar (3 volumes): extracted 1 file; deleted 3 archive files",
        ]
    );
}

#[tokio::test]
async fn extract_with_the_password_from_the_nzb() {
    let library = post(&["rar5-header-password/hidden.rar"]);
    let nzb = with_password(&library.nzb(), PASSWORD);

    let (outcome, _dir) = run(library, "hidden", nzb).await;

    assert_eq!(outcome.status, Status::Completed, "{:?}", outcome.error);
    assert_eq!(files_in(&outcome.folder), ["hidden.mkv"]);
}

#[tokio::test]
async fn extract_with_the_password_from_the_job_name() {
    let library = post(&["7z-password/secret.7z"]);
    let nzb = library.nzb();

    let (outcome, _dir) = run(library, &format!("secret{{{{{PASSWORD}}}}}"), nzb).await;

    assert_eq!(outcome.status, Status::Completed, "{:?}", outcome.error);
    assert_eq!(files_in(&outcome.folder), ["secret.mkv"]);
}

#[tokio::test]
async fn extract_without_a_password_fails_with_that_reason() {
    let library = post(&["rar5-password/secret.rar"]);
    let nzb = library.nzb();

    let (outcome, _dir) = run(library, "secret", nzb).await;

    assert_eq!(outcome.status, Status::Failed);
    let error = outcome.error.unwrap();
    assert!(
        error.starts_with("postproc-extract: secret.rar is password protected"),
        "{error}"
    );
    assert_eq!(files_in(&outcome.folder), ["secret.rar"]);
}

#[tokio::test]
async fn safety_blocks_a_download_with_an_executable_before_extraction() {
    let library = post(&["malicious/executable.rar"]);
    let nzb = library.nzb();

    let (outcome, _dir) = run(library, "show", nzb).await;

    assert_eq!(outcome.status, Status::Failed);
    assert_eq!(
        outcome.error.as_deref(),
        Some("postproc-safety: executable.rar contains `setup.exe`, a blocked file type")
    );
    assert_eq!(files_in(&outcome.folder), ["executable.rar"]);
    assert!(outcome.postprocess.is_empty());
}

#[tokio::test]
async fn safety_refuses_zip_slip_before_anything_is_written() {
    let library = post(&["malicious/zip-slip.7z"]);
    let nzb = library.nzb();

    let (outcome, dir) = run(library, "show", nzb).await;

    assert_eq!(outcome.status, Status::Failed);
    assert_eq!(
        outcome.error.as_deref(),
        Some(
            "postproc-safety: zip-slip.7z was refused: `Sample/../../evil.txt` climbs out of the \
             job folder with `..`"
        )
    );
    assert_eq!(files_in(&outcome.folder), ["zip-slip.7z"]);
    assert!(!dir.path().join("downloads").join("evil.txt").exists());
}
