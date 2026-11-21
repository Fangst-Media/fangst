//! Every request in `fixtures/adapters/qbittorrent/` goes through the adapter, backed by the jobs
//! the fixtures' README lists, and must give the recorded response shape. The README says how.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fangst_adapter_qbittorrent::Qbittorrent;
use fangst_providers::{
    AddNzb, AddNzbUrl, AddTorrent, ApiAdapter, BoxFuture, Credential, HttpRequest, HttpResponse,
    JobSummary, Jobs, LoginOutcome, ProviderError, TorrentFile, TorrentInput, TorrentStatus,
};
use serde_json::Value;

// Placeholders only; never real credentials.
const KEY: &str = "fixture-api-key-0000";
const PASSWORD: &str = "fixture-password-0000";
const SESSION: &str = "fixture-session-0000";
const ROOT: &str = "/data/downloads";

struct Entry {
    job: JobSummary,
    status: TorrentStatus,
}

struct FakeJobs {
    entries: Mutex<Vec<Entry>>,
    created: Mutex<Vec<String>>,
}

fn hash(n: u8) -> String {
    n.to_string().repeat(40)
}

fn job(id: i64, kind: &str, name: &str, category: &str, status: &str, bytes: i64) -> JobSummary {
    JobSummary {
        id,
        kind: kind.to_owned(),
        name: name.to_owned(),
        category: (!category.is_empty()).then(|| category.to_owned()),
        priority: 0,
        status: status.to_owned(),
        error: None,
        output_path: None,
        total_bytes: bytes,
        downloaded_bytes: 0,
        rate_bytes_per_sec: 0,
        eta_seconds: None,
        added_at: 1_732_190_000,
        finished_at: None,
    }
}

fn status(n: u8, files: &[(&str, u64)], have: u64, down: u64) -> TorrentStatus {
    TorrentStatus {
        info_hash: hash(n),
        files: files
            .iter()
            .map(|(path, bytes)| TorrentFile {
                path: (*path).to_owned(),
                bytes: *bytes,
            })
            .collect(),
        have_bytes: have,
        uploaded_bytes: 0,
        download_bps: down,
        upload_bps: 0,
        seeding: false,
        seeding_secs: 0,
        ratio_limit: Some(1.0),
        seeding_time_limit_secs: Some(604_800),
    }
}

fn in_folder(mut job: JobSummary) -> JobSummary {
    job.output_path = Some(PathBuf::from(format!("{ROOT}/{}-{}", job.id, job.name)));
    job
}

const EPISODE: u64 = 1_468_006_400;

impl FakeJobs {
    fn seeded() -> Arc<Self> {
        let e1 = "Show.S01E01.1080p.WEB-DL";
        let e2 = "Show.S01E02.1080p.WEB-DL";
        let e3 = "Show.S01E03.1080p.WEB-DL";
        let e4 = "Show.S01E04.1080p.WEB-DL";
        let mut completed = in_folder(job(
            4,
            "torrent",
            e3,
            "tv-sonarr",
            "completed",
            1_468_007_424,
        ));
        completed.finished_at = Some(1_732_190_900);
        let mut failed = in_folder(job(5, "torrent", e4, "tv-sonarr", "failed", 1_468_006_400));
        failed.error = Some("torrent engine: no peers".to_owned());
        failed.finished_at = Some(1_732_191_000);
        let mkv = |name: &str| format!("{name}.mkv");
        let entries = vec![
            Entry {
                // The queue's meter carries the progress; the engine's own numbers lag behind.
                job: JobSummary {
                    downloaded_bytes: 734_003_200,
                    rate_bytes_per_sec: 2_097_152,
                    eta_seconds: Some(350),
                    ..in_folder(job(
                        1,
                        "torrent",
                        e1,
                        "tv-sonarr",
                        "downloading",
                        1_468_006_400,
                    ))
                },
                status: status(1, &[(&mkv(e1), EPISODE)], 0, 0),
            },
            Entry {
                job: in_folder(job(2, "torrent", e2, "tv-sonarr", "paused", 1_468_006_400)),
                status: status(2, &[(&mkv(e2), EPISODE)], 0, 0),
            },
            Entry {
                job: job(
                    3,
                    "torrent",
                    "Movie.2020.1080p.BluRay",
                    "radarr",
                    "queued",
                    0,
                ),
                status: status(3, &[], 0, 0),
            },
            Entry {
                job: completed,
                status: status(
                    4,
                    &[(&mkv(e3), EPISODE), (&format!("{e3}.nfo"), 1024)],
                    0,
                    0,
                ),
            },
            Entry {
                job: failed,
                status: status(5, &[(&mkv(e4), EPISODE)], 0, 0),
            },
            Entry {
                job: job(6, "usenet", "Usenet.Job", "tv-sonarr", "queued", 1_000),
                status: status(6, &[], 0, 0),
            },
        ];
        Arc::new(Self {
            entries: Mutex::new(entries),
            created: Mutex::new(vec!["radarr".to_owned(), "tv-sonarr".to_owned()]),
        })
    }

    fn get(&self, id: i64) -> Option<JobSummary> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.job.id == id)
            .map(|e| e.job.clone())
    }

    fn set_status(&self, id: i64, from: &[&str], to: &str) -> Result<JobSummary, ProviderError> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .iter_mut()
            .find(|e| e.job.id == id)
            .ok_or_else(|| ProviderError::new("job not found", false))?;
        if !from.contains(&entry.job.status.as_str()) {
            return Err(ProviderError::new("wrong state", false));
        }
        to.clone_into(&mut entry.job.status);
        Ok(entry.job.clone())
    }
}

fn magnet_parts(link: &str) -> Option<(String, Option<String>)> {
    let query = link.strip_prefix("magnet:?")?;
    let mut hash = None;
    let mut name = None;
    for (key, value) in form_urlencoded_pairs(query) {
        match key.as_str() {
            "xt" => hash = value.strip_prefix("urn:btih:").map(str::to_ascii_lowercase),
            "dn" => name = Some(value),
            _ => {}
        }
    }
    Some((hash?, name))
}

fn form_urlencoded_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((k.to_owned(), v.replace("%20", " ")))
        })
        .collect()
}

fn not_supported<T: Send + 'static>() -> BoxFuture<Result<T, ProviderError>> {
    Box::pin(async { Err(ProviderError::new("not used by this adapter", false)) })
}

impl Jobs for FakeJobs {
    fn add_nzb(&self, _request: AddNzb) -> BoxFuture<Result<JobSummary, ProviderError>> {
        not_supported()
    }

    fn add_nzb_url(&self, _request: AddNzbUrl) -> BoxFuture<Result<JobSummary, ProviderError>> {
        not_supported()
    }

    fn list(&self) -> BoxFuture<Result<Vec<JobSummary>, ProviderError>> {
        // Newest first, as the queue lists them.
        let jobs: Vec<JobSummary> = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .rev()
            .map(|e| e.job.clone())
            .collect();
        Box::pin(async move { Ok(jobs) })
    }

    fn pause(&self, job_id: i64) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let result = self.set_status(job_id, &["queued", "downloading"], "paused");
        Box::pin(async move { result })
    }

    fn resume(&self, job_id: i64) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let result = self.set_status(job_id, &["paused", "failed"], "queued");
        Box::pin(async move { result })
    }

    fn delete(&self, job_id: i64, _delete_files: bool) -> BoxFuture<Result<(), ProviderError>> {
        self.entries.lock().unwrap().retain(|e| e.job.id != job_id);
        Box::pin(async { Ok(()) })
    }

    fn download_root(&self) -> PathBuf {
        PathBuf::from(ROOT)
    }

    fn add_torrent(&self, request: AddTorrent) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let (info_hash, from_link) = match &request.input {
            TorrentInput::Magnet(link) => match magnet_parts(link) {
                Some(parts) => parts,
                None => return Box::pin(async { Err(ProviderError::new("bad magnet", false)) }),
            },
            TorrentInput::File(_) => (hash(7), None),
            TorrentInput::Url(_) => return not_supported(),
        };
        let mut entries = self.entries.lock().unwrap();
        if entries.iter().any(|e| e.status.info_hash == info_hash) {
            return Box::pin(async { Err(ProviderError::new("duplicate", false)) });
        }
        let id = entries.iter().map(|e| e.job.id).max().unwrap_or(0) + 1;
        let name = request
            .name
            .or(from_link)
            .unwrap_or_else(|| "Untitled".to_owned());
        let mut new = job(id, "torrent", &name, "", "queued", 0);
        new.category = request.category;
        let mut status = status(7, &[], 0, 0);
        status.info_hash = info_hash;
        entries.push(Entry {
            job: new.clone(),
            status,
        });
        Box::pin(async move { Ok(new) })
    }

    fn torrent(&self, job_id: i64) -> BoxFuture<Result<Option<TorrentStatus>, ProviderError>> {
        let status = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.job.id == job_id && e.job.kind == "torrent")
            .map(|e| e.status.clone());
        Box::pin(async move { Ok(status) })
    }

    fn categories(&self) -> BoxFuture<Result<Vec<String>, ProviderError>> {
        let mut all = self.created.lock().unwrap().clone();
        all.extend(
            self.entries
                .lock()
                .unwrap()
                .iter()
                .filter_map(|e| e.job.category.clone()),
        );
        all.sort();
        all.dedup();
        Box::pin(async move { Ok(all) })
    }

    fn create_category(&self, name: String) -> BoxFuture<Result<(), ProviderError>> {
        let mut created = self.created.lock().unwrap();
        if !created.contains(&name) {
            created.push(name);
        }
        Box::pin(async { Ok(()) })
    }

    fn set_category(
        &self,
        job_id: i64,
        category: Option<String>,
    ) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let mut entries = self.entries.lock().unwrap();
        let result = entries
            .iter_mut()
            .find(|e| e.job.id == job_id)
            .map(|e| {
                e.job.category = category;
                e.job.clone()
            })
            .ok_or_else(|| ProviderError::new("job not found", false));
        Box::pin(async move { result })
    }
}

/// What the daemon does around the adapter: check the credential, sign in, then hand over.
async fn send(request: HttpRequest, jobs: Arc<FakeJobs>) -> HttpResponse {
    let adapter = Qbittorrent;
    match adapter.credential(&request) {
        Credential::Missing => adapter.rejected(&request, true),
        Credential::Login { username, password } => {
            let ok = password == KEY || (username == "admin" && password == PASSWORD);
            let outcome = if ok {
                LoginOutcome::Accepted {
                    session: SESSION.to_owned(),
                    lifetime_secs: 3600,
                }
            } else {
                LoginOutcome::Refused
            };
            adapter.logged_in(&request, outcome)
        }
        Credential::ApiKey(key) if key != KEY => adapter.rejected(&request, false),
        Credential::Session(token) if token != SESSION => adapter.rejected(&request, false),
        Credential::ApiKey(_) | Credential::Session(_) | Credential::NotNeeded => {
            adapter.handle(request, jobs).await
        }
    }
}

fn pairs(value: Option<&Value>) -> Vec<(String, String)> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().to_owned(),
                pair[1].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn request(fixture: &Value) -> HttpRequest {
    let r = &fixture["request"];
    HttpRequest {
        method: r["method"].as_str().unwrap().to_owned(),
        path: r["path"].as_str().unwrap().to_owned(),
        query: pairs(r.get("query")),
        headers: pairs(r.get("headers")),
        body: r["body"].as_str().unwrap_or_default().as_bytes().to_vec(),
    }
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "integer",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Every key in `expected` exists in `actual` with the same JSON type, integers and floats told
/// apart since Go clients refuse `1.0` where they expect an integer. Array items and object
/// values keyed by hash are checked against the expected first one.
fn same_shape(expected: &Value, actual: &Value, at: &str, problems: &mut Vec<String>) {
    if kind(expected) != kind(actual) {
        problems.push(format!(
            "{at}: expected {}, got {}",
            kind(expected),
            kind(actual)
        ));
        return;
    }
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) if at.ends_with("/torrents") => {
            if let Some(first) = e.values().next() {
                if a.is_empty() {
                    problems.push(format!("{at}: expected entries, got none"));
                }
                for (key, item) in a {
                    same_shape(first, item, &format!("{at}/{key}"), problems);
                }
            }
        }
        (Value::Object(e), Value::Object(a)) => {
            for (key, value) in e {
                match a.get(key) {
                    Some(found) => same_shape(value, found, &format!("{at}/{key}"), problems),
                    None => problems.push(format!("{at}/{key}: missing")),
                }
            }
        }
        (Value::Array(e), Value::Array(a)) => {
            if let Some(first) = e.first() {
                if a.is_empty() {
                    problems.push(format!("{at}: expected items, got none"));
                }
                for (i, item) in a.iter().enumerate() {
                    same_shape(first, item, &format!("{at}/{i}"), problems);
                }
            }
        }
        _ => {}
    }
}

/// Paths come out with the platform's separator; fixtures are written with `/`.
fn portable(value: Option<&Value>) -> Option<Value> {
    value.map(|v| match v {
        Value::String(s) => Value::String(s.replace('\\', "/")),
        other => other.clone(),
    })
}

fn fixture_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    Path::new(&manifest).join("../../fixtures/adapters/qbittorrent")
}

fn header<'a>(response: &'a HttpResponse, name: &str) -> &'a str {
    response
        .headers
        .iter()
        .find(|(k, _)| k == name)
        .map_or_default(|(_, v)| v.as_str())
}

fn check_body(fixture: &Value, response: &HttpResponse, problems: &mut Vec<String>) {
    let expected = &fixture["response"];
    let content_type = header(response, "content-type");
    if expected["content_type"] == "text" {
        if !content_type.starts_with("text/plain") {
            problems.push(format!("content type {content_type}"));
        }
        if response.body != expected["body"].as_str().unwrap().as_bytes() {
            problems.push(format!(
                "body {:?}",
                String::from_utf8_lossy(&response.body)
            ));
        }
        return;
    }
    if !content_type.starts_with("application/json") {
        problems.push(format!("content type {content_type}"));
    }
    let actual: Value = match serde_json::from_slice(&response.body) {
        Ok(actual) => actual,
        Err(e) => return problems.push(format!("not JSON: {e}")),
    };
    same_shape(&expected["body"], &actual, "", problems);
    for pointer in fixture["exact"].as_array().into_iter().flatten() {
        let pointer = pointer.as_str().unwrap();
        let want = portable(expected["body"].pointer(pointer));
        let got = portable(actual.pointer(pointer));
        if want != got {
            problems.push(format!("{pointer}: expected {want:?}, got {got:?}"));
        }
    }
}

fn check_jobs(fixture: &Value, jobs: &FakeJobs, problems: &mut Vec<String>) {
    if let Some(added) = fixture.get("added") {
        let Some(new) = jobs.get(7) else {
            return problems.push("no job was added".to_owned());
        };
        let got = serde_json::json!({
            "name": new.name, "category": new.category, "status": new.status, "kind": new.kind,
        });
        if &got != added {
            problems.push(format!("added {got}"));
        }
    } else if jobs.get(7).is_some() {
        problems.push("a job was added".to_owned());
    }
    for id in fixture["gone"].as_array().into_iter().flatten() {
        if jobs.get(id.as_i64().unwrap()).is_some() {
            problems.push(format!("job {id} still exists"));
        }
    }
    for (id, status) in fixture["statuses"].as_object().into_iter().flatten() {
        let job = jobs.get(id.parse().unwrap()).unwrap();
        if job.status != status.as_str().unwrap() {
            problems.push(format!("job {id} is {}", job.status));
        }
    }
    for (id, category) in fixture["job_categories"].as_object().into_iter().flatten() {
        let job = jobs.get(id.parse().unwrap()).unwrap();
        if job.category.as_deref() != category.as_str() {
            problems.push(format!("job {id} is in {:?}", job.category));
        }
    }
    if let Some(expected) = fixture.get("categories") {
        let mut created = jobs.created.lock().unwrap().clone();
        created.sort();
        if &serde_json::json!(created) != expected {
            problems.push(format!("categories {created:?}"));
        }
    }
}

async fn check(path: &Path) -> Vec<String> {
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let jobs = FakeJobs::seeded();
    let response = send(request(&fixture), Arc::clone(&jobs)).await;
    let expected = &fixture["response"];
    let mut problems = Vec::new();
    if u64::from(response.status) != expected["status"].as_u64().unwrap() {
        problems.push(format!(
            "status {} ({})",
            response.status,
            String::from_utf8_lossy(&response.body)
        ));
    }
    for (name, prefix) in pairs(expected.get("headers")) {
        if !header(&response, &name).starts_with(&prefix) {
            problems.push(format!("{name}: {:?}", header(&response, &name)));
        }
    }
    check_body(&fixture, &response, &mut problems);
    check_jobs(&fixture, &jobs, &mut problems);
    problems
}

#[tokio::test]
async fn every_fixture_gets_the_recorded_shape() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 30, "fixtures went missing: {paths:?}");
    let mut failures = Vec::new();
    for path in &paths {
        let problems = check(path).await;
        if !problems.is_empty() {
            failures.push(format!(
                "{}:\n  {}",
                path.file_name().unwrap().to_string_lossy(),
                problems.join("\n  ")
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
