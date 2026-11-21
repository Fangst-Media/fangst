//! Every request in `fixtures/adapters/sabnzbd/` goes through the adapter, backed by the jobs the
//! fixtures' README lists, and must give the recorded response shape. The README says how.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fangst_adapter_sabnzbd::Sabnzbd;
use fangst_providers::{
    AddNzb, AddNzbUrl, ApiAdapter, BoxFuture, Credential, HttpRequest, HttpResponse, JobSummary,
    Jobs, ProviderError,
};
use serde_json::Value;

// Placeholder only; never a real credential.
const KEY: &str = "fixture-api-key-0000";

#[derive(Default)]
struct FakeJobs {
    jobs: Mutex<Vec<JobSummary>>,
}

fn job(id: i64, name: &str, category: &str, status: &str, priority: i64, bytes: i64) -> JobSummary {
    JobSummary {
        id,
        kind: "usenet".to_owned(),
        name: name.to_owned(),
        category: Some(category.to_owned()),
        priority,
        status: status.to_owned(),
        error: None,
        output_path: None,
        total_bytes: bytes,
        downloaded_bytes: 0,
        rate_bytes_per_sec: 0,
        eta_seconds: None,
        added_at: 1_469_171_000 + id * 1_000,
        finished_at: None,
    }
}

impl FakeJobs {
    fn seeded() -> Arc<Self> {
        let mut completed = job(
            4,
            "TV.Show.S04E02.720p.BluRay.x264-xHD",
            "tv",
            "completed",
            0,
            2_436_906_376,
        );
        completed.added_at = 1_469_172_000;
        completed.finished_at = Some(1_469_172_988);
        completed.output_path = Some(PathBuf::from(
            "/data/downloads/4-TV.Show.S04E02.720p.BluRay.x264-xHD",
        ));
        let mut failed = job(
            5,
            "TV.Show.S04E03.720p.BluRay.x264-xHD",
            "tv",
            "failed",
            0,
            2_436_906_376,
        );
        failed.added_at = 1_469_172_100;
        failed.finished_at = Some(1_469_173_100);
        failed.error =
            Some("12 segments missing on the server, 0 could not be downloaded".to_owned());
        failed.output_path = Some(PathBuf::from(
            "/data/downloads/5-TV.Show.S04E03.720p.BluRay.x264-xHD",
        ));
        // Matches the fixture's 1296.02 KB/s and 0:16:44 left.
        let mut downloading = job(
            1,
            "TV.Show.S04E11.720p.HDTV.x264",
            "tv",
            "downloading",
            0,
            1_339_755_315,
        );
        downloading.downloaded_bytes = 7_600_000;
        downloading.rate_bytes_per_sec = 1_327_124;
        downloading.eta_seconds = Some(1004);
        Arc::new(Self {
            jobs: Mutex::new(vec![
                downloading,
                job(
                    2,
                    "TV.Show.S04E12.720p.HDTV.x264",
                    "tv",
                    "paused",
                    0,
                    1_339_870_720,
                ),
                job(
                    3,
                    "Movie.2020.1080p.BluRay.x264",
                    "movies",
                    "queued",
                    1,
                    8_589_934_592,
                ),
                completed,
                failed,
            ]),
        })
    }

    fn get(&self, id: i64) -> Option<JobSummary> {
        self.jobs
            .lock()
            .unwrap()
            .iter()
            .find(|j| j.id == id)
            .cloned()
    }

    fn push(&self, name: Option<String>, category: Option<String>, priority: i64) -> JobSummary {
        let mut jobs = self.jobs.lock().unwrap();
        let id = jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
        let mut new = job(id, "", "", "queued", priority, 0);
        new.name = name.unwrap_or_else(|| "Untitled".to_owned());
        new.category = category;
        jobs.push(new.clone());
        new
    }

    fn set_status(&self, id: i64, from: &[&str], to: &str) -> Result<JobSummary, ProviderError> {
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or_else(|| ProviderError::new("job not found", false))?;
        if !from.contains(&job.status.as_str()) {
            return Err(ProviderError::new("wrong state", false));
        }
        to.clone_into(&mut job.status);
        Ok(job.clone())
    }
}

impl Jobs for FakeJobs {
    fn add_nzb(&self, request: AddNzb) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let added = self.push(request.name, request.category, request.priority);
        Box::pin(async move { Ok(added) })
    }

    fn add_nzb_url(&self, request: AddNzbUrl) -> BoxFuture<Result<JobSummary, ProviderError>> {
        let added = self.push(request.name, request.category, request.priority);
        Box::pin(async move { Ok(added) })
    }

    fn list(&self) -> BoxFuture<Result<Vec<JobSummary>, ProviderError>> {
        let jobs = self.jobs.lock().unwrap().clone();
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
        let mut jobs = self.jobs.lock().unwrap();
        let before = jobs.len();
        jobs.retain(|j| j.id != job_id);
        let result = if jobs.len() < before {
            Ok(())
        } else {
            Err(ProviderError::new("job not found", false))
        };
        Box::pin(async move { result })
    }

    fn download_root(&self) -> PathBuf {
        PathBuf::from("/data/downloads")
    }
}

/// What the daemon does around the adapter: check the credential, then hand over.
async fn send(request: HttpRequest, jobs: Arc<FakeJobs>) -> HttpResponse {
    let adapter = Sabnzbd;
    match adapter.credential(&request) {
        Credential::Missing => adapter.rejected(&request, true),
        Credential::ApiKey(key) if key != KEY => adapter.rejected(&request, false),
        Credential::ApiKey(_) | Credential::NotNeeded => adapter.handle(request, jobs).await,
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
        path: "/".to_owned(),
        query: pairs(r.get("query")),
        headers: pairs(r.get("headers")),
        body: r["body"].as_str().unwrap_or_default().as_bytes().to_vec(),
    }
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Every key in `expected` exists in `actual` with the same JSON type; array items are checked
/// against the expected first item.
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

fn fixture_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    Path::new(&manifest).join("../../fixtures/adapters/sabnzbd")
}

async fn check(path: &Path) -> Vec<String> {
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let jobs = FakeJobs::seeded();
    let response = send(request(&fixture), Arc::clone(&jobs)).await;
    let expected = &fixture["response"];
    let mut problems = Vec::new();
    if u64::from(response.status) != expected["status"].as_u64().unwrap() {
        problems.push(format!("status {}", response.status));
    }
    let content_type = response
        .headers
        .iter()
        .find(|(k, _)| k == "content-type")
        .map(|(_, v)| v.as_str())
        .unwrap_or_default();
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
        return problems;
    }
    if !content_type.starts_with("application/json") {
        problems.push(format!("content type {content_type}"));
    }
    let actual: Value = match serde_json::from_slice(&response.body) {
        Ok(actual) => actual,
        Err(e) => return vec![format!("not JSON: {e}")],
    };
    same_shape(&expected["body"], &actual, "", &mut problems);
    for pointer in fixture["exact"].as_array().into_iter().flatten() {
        let pointer = pointer.as_str().unwrap();
        if expected["body"].pointer(pointer) != actual.pointer(pointer) {
            problems.push(format!(
                "{pointer}: expected {:?}, got {:?}",
                expected["body"].pointer(pointer),
                actual.pointer(pointer)
            ));
        }
    }
    if let Some(added) = fixture.get("added") {
        let new = jobs.get(6).expect("a job was added");
        let got = serde_json::json!({
            "name": new.name, "category": new.category, "priority": new.priority, "status": new.status,
        });
        if &got != added {
            problems.push(format!("added {got}"));
        }
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
    assert!(paths.len() >= 15, "fixtures went missing: {paths:?}");
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

#[tokio::test]
async fn the_key_may_come_in_a_form_body() {
    let request = HttpRequest {
        method: "POST".to_owned(),
        path: "/".to_owned(),
        query: Vec::new(),
        headers: vec![(
            "content-type".to_owned(),
            "application/x-www-form-urlencoded".to_owned(),
        )],
        body: format!("mode=get_cats&apikey={KEY}").into_bytes(),
    };
    let response = send(request, FakeJobs::seeded()).await;
    assert_eq!(response.status, 200);
    assert!(String::from_utf8_lossy(&response.body).contains("categories"));
}

#[tokio::test]
async fn an_empty_key_counts_as_missing() {
    let request = HttpRequest {
        method: "GET".to_owned(),
        path: "/".to_owned(),
        query: vec![
            ("mode".to_owned(), "queue".to_owned()),
            ("apikey".to_owned(), String::new()),
        ],
        headers: Vec::new(),
        body: Vec::new(),
    };
    let response = send(request, FakeJobs::seeded()).await;
    assert_eq!(
        (response.status, response.body.as_slice()),
        (403, &b"API Key Required"[..])
    );
}

#[tokio::test]
async fn deleting_an_unknown_id_reports_false() {
    let request = HttpRequest {
        method: "GET".to_owned(),
        path: "/".to_owned(),
        query: [
            ("mode", "queue"),
            ("name", "delete"),
            ("value", "SABnzbd_nzo_99"),
            ("apikey", KEY),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect(),
        headers: Vec::new(),
        body: Vec::new(),
    };
    let response = send(request, FakeJobs::seeded()).await;
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body, serde_json::json!({ "status": false, "nzo_ids": [] }));
}
