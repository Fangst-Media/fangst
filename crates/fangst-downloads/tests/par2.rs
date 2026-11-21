use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fangst_bench::{Config, Library, MockServer};
use fangst_downloads::{Runner, Status, Store};
use fangst_nntp::{Pool, ServerConfig};
use fangst_providers::{
    BoxFuture, JobContext, PostProcessOutcome, PostProcessor, ProviderError, ProviderInfo,
    ProviderKind,
};

const SLICE: usize = 4096;
const SEGMENT: usize = 4000;
const DATA_LEN: usize = 160_000;
/// Volumes of 1, 2, 4 and 8 blocks: 15 recovery blocks in all.
const VOLUMES: [u32; 4] = [1, 2, 4, 8];

fn data(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| {
            let x = u32::try_from(i).unwrap().wrapping_mul(2_654_435_761);
            (x >> 24).to_le_bytes()[0] | 1
        })
        .collect()
}

/// A post of `show.bin` with an index `.par2` and four recovery volumes, every file split into
/// articles. Returns the library, the payload and each file's message ids by name.
fn post() -> (Library, Vec<u8>, HashMap<String, Vec<String>>) {
    let payload = data(DATA_LEN);
    let mut library = Library::new();
    let mut ids = HashMap::new();
    ids.insert(
        "show.bin".to_owned(),
        library.add_file("show.bin", &payload, SEGMENT, "alt.binaries.test"),
    );
    for (name, contents) in
        fangst_bench::par2::create("show", &[("show.bin", &payload)], SLICE, &VOLUMES)
    {
        let file_ids = library.add_file(&name, &contents, SEGMENT, "alt.binaries.test");
        ids.insert(name, file_ids);
    }
    (library, payload, ids)
}

fn runner(store: Store, server: &MockServer, root: &Path) -> Runner {
    Runner {
        store,
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
        root: root.to_owned(),
        options: fangst_usenet::Options {
            connections: 2,
            pipeline_depth: 4,
            retries: 1,
        },
        post_processors: Vec::new(),
    }
}

struct Outcome {
    status: Status,
    error: Option<String>,
    folder: PathBuf,
    events: Vec<fangst_downloads::Event>,
}

async fn run(
    library: Library,
    missing: HashSet<String>,
    post: Vec<Arc<dyn PostProcessor>>,
) -> (Outcome, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let id = store
        .add_nzb("show", Some("tv"), 0, library.nzb().as_bytes())
        .await
        .unwrap();
    let config = Config {
        missing,
        ..Config::default()
    };
    let server = MockServer::start(library, config).await.unwrap();
    let mut runner = runner(store.clone(), &server, &dir.path().join("downloads"));
    runner.post_processors = post;

    runner.run_until_idle().await.unwrap();

    let job = store.get(id).await.unwrap();
    let outcome = Outcome {
        status: job.status,
        error: job.error,
        folder: job.output_path.unwrap(),
        events: store.timeline(id).await.unwrap(),
    };
    (outcome, dir)
}

fn messages<'a>(events: &'a [fangst_downloads::Event], kind: &str) -> Vec<&'a str> {
    events
        .iter()
        .filter(|e| e.kind == kind)
        .map(|e| e.message.as_str())
        .collect()
}

fn volumes_on_disk(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".vol"))
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn par2_intact_download_never_fetches_recovery_volumes() {
    let (library, payload, _) = post();

    let (outcome, _dir) = run(library, HashSet::new(), Vec::new()).await;

    assert_eq!(outcome.status, Status::Completed, "{:?}", outcome.error);
    assert_eq!(
        std::fs::read(outcome.folder.join("show.bin")).unwrap(),
        payload
    );
    assert!(volumes_on_disk(&outcome.folder).is_empty());
    assert_eq!(
        messages(&outcome.events, "verify"),
        ["show.par2: all 1 files intact"]
    );
    assert!(messages(&outcome.events, "repair").is_empty());
}

#[tokio::test]
async fn par2_five_percent_missing_articles_are_repaired() {
    let (library, payload, ids) = post();
    let data_ids = &ids["show.bin"];
    // 2 of 40 articles. Articles 6 and 21 each straddle a block boundary: 4 damaged blocks.
    let missing = HashSet::from([data_ids[5].clone(), data_ids[20].clone()]);

    let (outcome, _dir) = run(library, missing, Vec::new()).await;

    assert_eq!(outcome.status, Status::Completed, "{:?}", outcome.error);
    assert_eq!(
        std::fs::read(outcome.folder.join("show.bin")).unwrap(),
        payload
    );
    // Exactly the one volume with 4 blocks, nothing more.
    assert_eq!(volumes_on_disk(&outcome.folder), ["show.vol03+04.par2"]);
    assert_eq!(
        messages(&outcome.events, "verify"),
        ["show.par2: show.bin has 4 damaged blocks; 4 recovery blocks needed"]
    );
    assert_eq!(
        messages(&outcome.events, "repair"),
        [
            "fetching 1 recovery volumes with 4 blocks for show.par2",
            "show.par2: repaired 4 blocks in 1 files"
        ]
    );
}

#[tokio::test]
async fn par2_damaged_recovery_volume_fetches_more() {
    let (library, payload, ids) = post();
    let data_ids = &ids["show.bin"];
    let missing = HashSet::from([
        data_ids[5].clone(),
        data_ids[20].clone(),
        // The first article of the 4-block volume carries its first recovery block.
        ids["show.vol03+04.par2"][0].clone(),
    ]);

    let (outcome, _dir) = run(library, missing, Vec::new()).await;

    assert_eq!(outcome.status, Status::Completed, "{:?}", outcome.error);
    assert_eq!(
        std::fs::read(outcome.folder.join("show.bin")).unwrap(),
        payload
    );
    let fetched = messages(&outcome.events, "repair");
    assert_eq!(fetched.len(), 3, "{fetched:?}");
    assert_eq!(fetched[2], "show.par2: repaired 4 blocks in 1 files");
}

#[tokio::test]
async fn par2_not_enough_recovery_blocks_fails_with_that_reason() {
    let (library, _, ids) = post();
    let data_ids = &ids["show.bin"];
    // Every other article is gone: far more damaged blocks than the 15 recovery blocks.
    let missing: HashSet<String> = data_ids.iter().step_by(2).cloned().collect();

    let (outcome, _dir) = run(library, missing, Vec::new()).await;

    assert_eq!(outcome.status, Status::Failed);
    let reason = outcome.error.unwrap();
    assert!(
        reason.starts_with("PAR2 repair needs ") && reason.ends_with(", only 15 are available"),
        "{reason}"
    );
    assert!(
        volumes_on_disk(&outcome.folder).is_empty(),
        "volumes were fetched although they could never be enough"
    );
    assert_eq!(
        outcome.events.last().unwrap().message,
        format!("failed: {reason}")
    );
}

struct Step {
    id: &'static str,
    outcome: PostProcessOutcome,
    seen: Arc<Mutex<Vec<(String, JobContext)>>>,
}

impl PostProcessor for Step {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.id.to_owned(),
            kind: ProviderKind::PostProcessor,
            description: "test step".to_owned(),
        }
    }

    fn process(&self, job: JobContext) -> BoxFuture<Result<PostProcessOutcome, ProviderError>> {
        self.seen.lock().unwrap().push((self.id.to_owned(), job));
        let outcome = self.outcome.clone();
        Box::pin(async move { Ok(outcome) })
    }
}

fn step(
    id: &'static str,
    outcome: PostProcessOutcome,
    seen: &Arc<Mutex<Vec<(String, JobContext)>>>,
) -> Arc<dyn PostProcessor> {
    Arc::new(Step {
        id,
        outcome,
        seen: Arc::clone(seen),
    })
}

#[tokio::test]
async fn post_processors_run_in_order_after_repair() {
    let (library, _, ids) = post();
    let missing = HashSet::from([ids["show.bin"][5].clone()]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let steps = vec![
        step(
            "first",
            PostProcessOutcome::Continue {
                message: "unpacked 1 archive".to_owned(),
            },
            &seen,
        ),
        step(
            "second",
            PostProcessOutcome::Continue {
                message: "nothing unsafe".to_owned(),
            },
            &seen,
        ),
    ];

    let (outcome, _dir) = run(library, missing, steps).await;

    assert_eq!(outcome.status, Status::Completed, "{:?}", outcome.error);
    let seen = seen.lock().unwrap();
    let order: Vec<&str> = seen.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(order, ["first", "second"]);
    let context = &seen[0].1;
    assert_eq!(context.name, "show");
    assert_eq!(context.category.as_deref(), Some("tv"));
    assert_eq!(context.folder, outcome.folder);
    assert_eq!(
        messages(&outcome.events, "postprocess"),
        ["first: unpacked 1 archive", "second: nothing unsafe"]
    );
    // Post-processing starts only after the repair.
    let kinds: Vec<&str> = outcome.events.iter().map(|e| e.kind.as_str()).collect();
    let repaired = kinds.iter().rposition(|k| *k == "repair").unwrap();
    let first_post = kinds.iter().position(|k| *k == "postprocess").unwrap();
    assert!(repaired < first_post, "{kinds:?}");
}

#[tokio::test]
async fn failing_post_processor_fails_the_job_and_stops_the_chain() {
    let (library, _, _) = post();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let steps = vec![
        step(
            "unpack",
            PostProcessOutcome::Fail {
                reason: "archive is encrypted and no password is known".to_owned(),
            },
            &seen,
        ),
        step(
            "never",
            PostProcessOutcome::Continue {
                message: "ran".to_owned(),
            },
            &seen,
        ),
    ];

    let (outcome, _dir) = run(library, HashSet::new(), steps).await;

    assert_eq!(outcome.status, Status::Failed);
    assert_eq!(
        outcome.error.as_deref(),
        Some("unpack: archive is encrypted and no password is known")
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn post_processors_do_not_run_when_the_download_failed() {
    let (library, _, ids) = post();
    let missing: HashSet<String> = ids["show.bin"].iter().step_by(2).cloned().collect();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let steps = vec![step(
        "unpack",
        PostProcessOutcome::Continue {
            message: "ran".to_owned(),
        },
        &seen,
    )];

    let (outcome, _dir) = run(library, missing, steps).await;

    assert_eq!(outcome.status, Status::Failed);
    assert!(seen.lock().unwrap().is_empty());
}
