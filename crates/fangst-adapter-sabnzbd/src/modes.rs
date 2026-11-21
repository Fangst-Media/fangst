//! One function per SABnzbd `mode`. Field names, types and formats follow SABnzbd 4.5; the
//! clients parse strictly in places (`HolyArrs` wants `mb` as a string, Sonarr throws on a status
//! outside its enum), so every value keeps SABnzbd's JSON type even where Fangst has no data.

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use fangst_providers::{AddNzb, AddNzbUrl, HttpResponse, JobSummary, Jobs, ProviderError};
use serde_json::{Value, json};

use crate::params::Params;

/// Sonarr wants three dot-separated numbers and at least 0.7; a release that exists keeps any
/// client-side version gate on known ground.
pub const SAB_VERSION: &str = "4.5.5";

/// SABnzbd's own default categories. Sonarr's connection test fails unless its category exists,
/// and Fangst has no category list of its own yet, so these plus every category in use are listed.
const DEFAULT_CATEGORIES: [&str; 4] = ["movies", "tv", "audio", "software"];

/// SABnzbd's fallback when `limit` is missing or 0 for history (`misc.history_limit`).
const DEFAULT_HISTORY_LIMIT: usize = 10;

const NZO_PREFIX: &str = "SABnzbd_nzo_";

const MEBIBYTE: f64 = 1_048_576.0;

const DAY: i64 = 24 * 60 * 60;

/// The SABnzbd id for a Fangst job. The mapping is a formula, so the adapter keeps no table.
#[must_use]
pub fn nzo_id(job_id: i64) -> String {
    format!("{NZO_PREFIX}{job_id}")
}

#[must_use]
pub fn parse_nzo_id(nzo_id: &str) -> Option<i64> {
    nzo_id.trim().strip_prefix(NZO_PREFIX)?.parse().ok()
}

fn ids(value: &str) -> Vec<i64> {
    value.split(',').filter_map(parse_nzo_id).collect()
}

fn ok_json(body: &Value) -> HttpResponse {
    HttpResponse::json(200, body)
}

/// SABnzbd reports bad input with HTTP 200 and `status: false`.
fn error(message: &str) -> HttpResponse {
    ok_json(&json!({ "status": false, "error": message }))
}

pub(crate) async fn dispatch(params: &Params, jobs: &dyn Jobs) -> HttpResponse {
    let result = match params.get("mode").unwrap_or_default() {
        "version" => Ok(ok_json(&json!({ "version": SAB_VERSION }))),
        "queue" => queue(params, jobs).await,
        "history" => history(params, jobs).await,
        "addfile" => add_file(params, jobs).await,
        "addurl" => add_url(params, jobs).await,
        "get_cats" => categories(jobs)
            .await
            .map(|cats| ok_json(&json!({ "categories": cats }))),
        "get_config" => get_config(params, jobs).await,
        "fullstatus" | "status" => full_status(jobs).await,
        "retry" => retry(params, jobs).await,
        _ => Ok(error("not implemented")),
    };
    result.unwrap_or_else(|e| error(&e.message))
}

fn is_queue(job: &JobSummary) -> bool {
    matches!(job.status.as_str(), "queued" | "downloading" | "paused")
}

fn is_history(job: &JobSummary) -> bool {
    matches!(job.status.as_str(), "completed" | "failed")
}

/// SABnzbd's `*` (and the `Default` label in its UI) means no category.
fn category_in(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|c| !c.is_empty() && *c != "*" && !c.eq_ignore_ascii_case("default"))
        .map(str::to_owned)
}

fn category_out(job: &JobSummary) -> &str {
    job.category.as_deref().unwrap_or("*")
}

/// Keeps the jobs a `category`/`cat` and `nzo_ids` filter asks for.
fn filtered<'a>(
    params: &Params,
    jobs: &'a [JobSummary],
    keep: fn(&JobSummary) -> bool,
) -> Vec<&'a JobSummary> {
    let category = params.non_empty("category").or(params.non_empty("cat"));
    let wanted = params.non_empty("nzo_ids").map(ids);
    jobs.iter()
        .filter(|job| keep(job))
        .filter(|job| category.is_none_or(|c| category_out(job) == c))
        .filter(|job| wanted.as_ref().is_none_or(|w| w.contains(&job.id)))
        .collect()
}

fn number(params: &Params, name: &str) -> usize {
    params
        .non_empty(name)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

fn page<'a>(items: &[&'a JobSummary], start: usize, limit: usize) -> Vec<&'a JobSummary> {
    let rest = items.iter().skip(start).copied();
    if limit == 0 {
        rest.collect()
    } else {
        rest.take(limit).collect()
    }
}

/// Display only; precision past 2^53 bytes does not matter for a size label.
#[allow(clippy::cast_precision_loss)]
fn as_f64(bytes: i64) -> f64 {
    bytes.max(0) as f64
}

fn mb(bytes: i64) -> String {
    format!("{:.2}", as_f64(bytes) / MEBIBYTE)
}

/// SABnzbd's `to_units`: powers of 1024, one decimal from M upward, none below.
fn to_units(bytes: i64, postfix: &str) -> String {
    const UNITS: [&str; 6] = ["", "K", "M", "G", "T", "P"];
    let mut value = as_f64(bytes);
    let mut n = 0;
    while value >= 1024.0 && n < UNITS.len() - 1 {
        value /= 1024.0;
        n += 1;
    }
    let decimals = usize::from(n > 1);
    let factor = if decimals == 1 { 10.0 } else { 1.0 };
    // Rounding can carry into the next unit, as 1023.96 K does; SABnzbd moves up a unit then.
    if (value * factor).round() / factor >= 1024.0 && n < UNITS.len() - 1 {
        value /= 1024.0;
        n += 1;
    }
    let decimals = usize::from(n > 1);
    if n == 0 && postfix.is_empty() {
        format!("{value:.0}")
    } else {
        format!("{value:.decimals$} {}{postfix}", UNITS[n])
    }
}

/// SABnzbd's `format_time_left`: `H:MM:SS`, with a day field in front from one day up. Sonarr
/// fails the whole queue on anything else, so an unknown time is `0:00:00` as in SABnzbd.
fn time_left(seconds: Option<i64>) -> String {
    let total = seconds.unwrap_or(0).max(0);
    let (days, rest) = (total / DAY, total % DAY);
    let (hours, minutes, secs) = (rest / 3600, rest % 3600 / 60, rest % 60);
    if days > 0 {
        format!("{days}:{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{hours}:{minutes:02}:{secs:02}")
    }
}

/// A whole number in a string; Sonarr fails on `"45.5"`.
fn percentage(downloaded: i64, total: i64) -> String {
    if total <= 0 {
        return "0".to_owned();
    }
    let done = i128::from(downloaded.clamp(0, total)) * 100 / i128::from(total);
    done.to_string()
}

fn bytes_left(job: &JobSummary) -> i64 {
    job.total_bytes.saturating_sub(job.downloaded_bytes).max(0)
}

fn kb_per_sec(rate: i64) -> String {
    format!("{:.2}", as_f64(rate) / 1024.0)
}

/// SABnzbd always puts a space before the unit, which is empty below 1 KiB.
fn speed(rate: i64) -> String {
    let text = to_units(rate, "");
    if text.contains(' ') {
        text
    } else {
        format!("{text} ")
    }
}

fn priority_name(priority: i64) -> &'static str {
    match priority {
        p if p >= 2 => "Force",
        1 => "High",
        0 => "Normal",
        _ => "Low",
    }
}

fn queue_status(job: &JobSummary) -> &'static str {
    match job.status.as_str() {
        "downloading" => "Downloading",
        "paused" => "Paused",
        _ => "Queued",
    }
}

fn queue_slot(index: usize, job: &JobSummary) -> Value {
    json!({
        "index": index,
        "nzo_id": nzo_id(job.id),
        "unpackopts": "3",
        "priority": priority_name(job.priority),
        "script": "None",
        "filename": job.name,
        "labels": [],
        "password": "",
        "cat": category_out(job),
        "mbleft": mb(bytes_left(job)),
        "mb": mb(job.total_bytes),
        "size": to_units(job.total_bytes, "B"),
        "sizeleft": to_units(bytes_left(job), "B"),
        "percentage": percentage(job.downloaded_bytes, job.total_bytes),
        "mbmissing": "0.0",
        "direct_unpack": null,
        "status": queue_status(job),
        "timeleft": time_left(job.eta_seconds),
        "avg_age": "-",
        "time_added": job.added_at,
    })
}

fn sum_bytes<'a>(jobs: impl Iterator<Item = &'a JobSummary>) -> i64 {
    jobs.map(|j| j.total_bytes.max(0))
        .fold(0, i64::saturating_add)
}

fn header(all: &[JobSummary]) -> serde_json::Map<String, Value> {
    let queued: Vec<&JobSummary> = all.iter().filter(|j| is_queue(j)).collect();
    let bytes = sum_bytes(queued.iter().copied());
    let left = queued
        .iter()
        .map(|j| bytes_left(j))
        .fold(0, i64::saturating_add);
    let rate = queued
        .iter()
        .filter(|j| j.status == "downloading")
        .map(|j| j.rate_bytes_per_sec.max(0))
        .fold(0, i64::saturating_add);
    let downloading = queued.iter().any(|j| j.status == "downloading");
    let Value::Object(map) = json!({
        "version": SAB_VERSION,
        "paused": false,
        "pause_int": "0",
        "paused_all": false,
        // Fangst does not measure free space for clients yet.
        "diskspace1": "0.00",
        "diskspace2": "0.00",
        "diskspace1_norm": "0 ",
        "diskspace2_norm": "0 ",
        "diskspacetotal1": "0.00",
        "diskspacetotal2": "0.00",
        "speedlimit": "100",
        "speedlimit_abs": "",
        "have_warnings": "0",
        "finishaction": null,
        "quota": "0 ",
        "have_quota": false,
        "left_quota": "0 ",
        "cache_art": "0",
        "cache_size": "0 B",
        "kbpersec": kb_per_sec(rate),
        "speed": speed(rate),
        "mbleft": mb(left),
        "mb": mb(bytes),
        "sizeleft": to_units(left, "B"),
        "size": to_units(bytes, "B"),
        "status": if downloading { "Downloading" } else { "Idle" },
        // Everything queued at the combined speed, as SABnzbd estimates it.
        "timeleft": time_left((rate > 0).then(|| left.saturating_add(rate - 1) / rate)),
    }) else {
        unreachable!("json! of an object literal is an object")
    };
    map
}

async fn queue(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    match params.get("name").unwrap_or_default() {
        "" => {}
        "delete" => return queue_delete(params, jobs).await,
        "pause" => return queue_toggle(params, jobs, true).await,
        "resume" => return queue_toggle(params, jobs, false).await,
        _ => return Ok(error("not implemented")),
    }
    let all = jobs.list().await?;
    let mut matching = filtered(params, &all, is_queue);
    // Queue order is run order: higher priority first, then oldest first.
    matching.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
    let start = number(params, "start");
    let slots = page(&matching, start, number(params, "limit"));
    let mut body = header(&all);
    body.insert("noofslots_total".to_owned(), json!(matching.len()));
    body.insert("noofslots".to_owned(), json!(slots.len()));
    body.insert("start".to_owned(), json!(start));
    body.insert("limit".to_owned(), json!(number(params, "limit")));
    body.insert("finish".to_owned(), json!(start + slots.len()));
    body.insert(
        "slots".to_owned(),
        Value::Array(
            slots
                .iter()
                .enumerate()
                .map(|(i, job)| queue_slot(start + i, job))
                .collect(),
        ),
    );
    Ok(ok_json(&json!({ "queue": body })))
}

fn wants_files(params: &Params) -> bool {
    params.get("del_files") == Some("1")
}

/// Ids named by `value`; `all` (and for history `failed`) select by state like SABnzbd does.
fn targets(params: &Params, all: &[JobSummary], keep: fn(&JobSummary) -> bool) -> Vec<i64> {
    let value = params.non_empty("value").unwrap_or_default();
    let selected: Vec<&JobSummary> = all.iter().filter(|j| keep(j)).collect();
    match value {
        "all" => selected.iter().map(|j| j.id).collect(),
        "failed" => selected
            .iter()
            .filter(|j| j.status == "failed")
            .map(|j| j.id)
            .collect(),
        _ => {
            let wanted = ids(value);
            selected
                .iter()
                .filter(|j| wanted.contains(&j.id))
                .map(|j| j.id)
                .collect()
        }
    }
}

async fn queue_delete(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    if params.non_empty("value").is_none() {
        return Ok(error("expects one parameter"));
    }
    let all = jobs.list().await?;
    let mut deleted = Vec::new();
    for id in targets(params, &all, is_queue) {
        if jobs.delete(id, wants_files(params)).await.is_ok() {
            deleted.push(nzo_id(id));
        }
    }
    Ok(ok_json(
        &json!({ "status": !deleted.is_empty(), "nzo_ids": deleted }),
    ))
}

async fn queue_toggle(
    params: &Params,
    jobs: &dyn Jobs,
    pause: bool,
) -> Result<HttpResponse, ProviderError> {
    let Some(value) = params.non_empty("value") else {
        return Ok(error("expects one parameter"));
    };
    let mut changed = Vec::new();
    for id in ids(value) {
        let result = if pause {
            jobs.pause(id).await
        } else {
            jobs.resume(id).await
        };
        if result.is_ok() {
            changed.push(nzo_id(id));
        }
    }
    Ok(ok_json(
        &json!({ "status": !changed.is_empty(), "nzo_ids": changed }),
    ))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn history_slot(job: &JobSummary) -> Value {
    let storage = job
        .output_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let completed = job.finished_at.unwrap_or(job.added_at);
    let failed = job.status == "failed";
    json!({
        "action_line": "",
        "duplicate_key": "",
        "meta": null,
        "fail_message": if failed { job.error.as_deref().unwrap_or("Failed") } else { "" },
        "loaded": false,
        "size": to_units(job.total_bytes, "B"),
        "category": category_out(job),
        "pp": "D",
        "retry": 0,
        "script": "None",
        "nzb_name": format!("{}.nzb", job.name),
        "download_time": (completed - job.added_at).max(0),
        "storage": storage,
        "status": if failed { "Failed" } else { "Completed" },
        "script_line": "",
        "completed": completed,
        "time_added": job.added_at,
        "nzo_id": nzo_id(job.id),
        "downloaded": if failed { 0 } else { job.total_bytes.max(0) },
        "report": "",
        "password": "",
        "path": storage,
        "postproc_time": 0,
        "name": job.name,
        "url": "",
        "md5sum": "",
        "archive": false,
        "bytes": job.total_bytes.max(0),
        "url_info": "",
        "stage_log": [],
    })
}

async fn history(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    match params.get("name").unwrap_or_default() {
        "" => {}
        "delete" => return history_delete(params, jobs).await,
        _ => return Ok(error("not implemented")),
    }
    let all = jobs.list().await?;
    let mut matching = filtered(params, &all, is_history);
    // Newest first, as SABnzbd lists history.
    matching.sort_by(|a, b| {
        b.finished_at
            .unwrap_or(b.added_at)
            .cmp(&a.finished_at.unwrap_or(a.added_at))
            .then(b.id.cmp(&a.id))
    });
    let limit = match number(params, "limit") {
        0 => DEFAULT_HISTORY_LIMIT,
        n => n,
    };
    let slots = page(&matching, number(params, "start"), limit);
    let finished: Vec<&JobSummary> = all.iter().filter(|j| is_history(j)).collect();
    let since = |seconds: i64| {
        let cutoff = now() - seconds;
        sum_bytes(
            finished
                .iter()
                .copied()
                .filter(|j| j.finished_at.unwrap_or(0) >= cutoff),
        )
    };
    Ok(ok_json(&json!({ "history": {
        "noofslots": matching.len(),
        "ppslots": 0,
        "day_size": to_units(since(DAY), ""),
        "week_size": to_units(since(7 * DAY), ""),
        "month_size": to_units(since(30 * DAY), ""),
        "total_size": to_units(sum_bytes(finished.iter().copied()), ""),
        // Derived, not stored: it moves whenever a job finishes. SABnzbd's counter starts at 1,
        // and clients that send it back always get the full history here.
        "last_history_update": finished.iter().filter_map(|j| j.finished_at).max().unwrap_or(0).max(1),
        "version": SAB_VERSION,
        "slots": slots.iter().map(|j| history_slot(j)).collect::<Vec<_>>(),
    }})))
}

async fn history_delete(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    if params.non_empty("value").is_none() {
        return Ok(error("expects one parameter"));
    }
    let all = jobs.list().await?;
    // Fangst keeps no archive, so `archive=1` removes the entry as `archive=0` does.
    for id in targets(params, &all, is_history) {
        jobs.delete(id, wants_files(params)).await?;
    }
    Ok(ok_json(&json!({ "status": true })))
}

/// `-100` is SABnzbd's "category default" and `-2` "add paused"; Fangst has neither, so
/// they become normal priority, and `-2` pauses the job right after adding.
fn priority_in(params: &Params) -> (i64, bool) {
    match params
        .non_empty("priority")
        .and_then(|p| p.trim().parse::<i64>().ok())
    {
        Some(-2) => (0, true),
        Some(p @ -1..=2) => (p, false),
        _ => (0, false),
    }
}

fn strip_nzb(name: &str) -> &str {
    match name
        .len()
        .checked_sub(4)
        .and_then(|cut| name.split_at_checked(cut))
    {
        Some((stem, ext)) if !stem.is_empty() && ext.eq_ignore_ascii_case(".nzb") => stem,
        _ => name,
    }
}

async fn added(
    result: Result<JobSummary, ProviderError>,
    paused: bool,
    jobs: &dyn Jobs,
) -> HttpResponse {
    match result {
        Ok(job) => {
            if paused {
                // The job is queued either way; a failed pause only means it starts sooner.
                let _ = jobs.pause(job.id).await;
            }
            ok_json(&json!({ "status": true, "nzo_ids": [nzo_id(job.id)] }))
        }
        Err(e) => ok_json(&json!({ "status": false, "error": e.message, "nzo_ids": [] })),
    }
}

async fn add_file(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let Some(upload) = params
        .uploads
        .iter()
        .find(|u| u.field == "name" || u.field == "nzbfile")
    else {
        return Ok(error("expects one parameter"));
    };
    let (priority, paused) = priority_in(params);
    let name = params.non_empty("nzbname").map(str::to_owned).or_else(|| {
        upload
            .filename
            .as_deref()
            .map(strip_nzb)
            .filter(|n| !n.trim().is_empty())
            .map(str::to_owned)
    });
    let result = jobs
        .add_nzb(AddNzb {
            name,
            category: category_in(params.get("cat")),
            priority,
            nzb: upload.bytes.clone(),
        })
        .await;
    Ok(added(result, paused, jobs).await)
}

async fn add_url(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let Some(url) = params.non_empty("name") else {
        return Ok(error("expects one parameter"));
    };
    let (priority, paused) = priority_in(params);
    let result = jobs
        .add_nzb_url(AddNzbUrl {
            url: url.trim().to_owned(),
            name: params.non_empty("nzbname").map(str::to_owned),
            category: category_in(params.get("cat")),
            priority,
        })
        .await;
    Ok(added(result, paused, jobs).await)
}

async fn categories(jobs: &dyn Jobs) -> Result<Vec<String>, ProviderError> {
    let mut names: BTreeSet<String> = DEFAULT_CATEGORIES.iter().map(|&c| c.to_owned()).collect();
    names.extend(jobs.list().await?.into_iter().filter_map(|j| j.category));
    Ok(std::iter::once("*".to_owned()).chain(names).collect())
}

fn category_object(order: usize, name: &str) -> Value {
    let default = name == "*";
    // An empty `dir` puts jobs straight in the complete folder, which is where Fangst puts them.
    json!({
        "name": name,
        "order": order,
        "pp": if default { "3" } else { "" },
        "script": if default { "None" } else { "Default" },
        "dir": "",
        "newzbin": "",
        "priority": if default { 0 } else { -100 },
    })
}

async fn get_config(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let root = jobs.download_root().to_string_lossy().into_owned();
    let misc = json!({
        "complete_dir": root,
        "download_dir": root,
        "pre_check": 0,
        "history_retention": "",
        // "all" tells Sonarr that finished jobs stay in history until it removes them.
        "history_retention_option": "all",
        "history_retention_number": 1,
    });
    let categories: Vec<Value> = categories(jobs)
        .await?
        .iter()
        .enumerate()
        .map(|(i, name)| category_object(i, name))
        .collect();
    let config = match params.non_empty("section") {
        None => json!({
            "misc": misc,
            "categories": categories,
            // Sonarr calls `.Any()` on this list, so it must be an array, never null.
            "sorters": [],
            "servers": [],
            "rss": [],
        }),
        Some("misc") => json!({ "misc": misc }),
        Some("categories") => json!({ "categories": categories }),
        Some(section @ ("sorters" | "servers" | "rss")) => json!({ section: [] }),
        Some(_) => return Ok(error("not implemented")),
    };
    Ok(ok_json(&json!({ "config": config })))
}

async fn full_status(jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let all = jobs.list().await?;
    let root = jobs.download_root().to_string_lossy().into_owned();
    let mut status = header(&all);
    for (key, value) in [
        ("completedir", json!(root)),
        ("downloaddir", json!(root)),
        ("servers", json!([])),
        ("warnings", json!([])),
        ("folders", json!([])),
    ] {
        status.insert(key.to_owned(), value);
    }
    Ok(ok_json(&json!({ "status": status })))
}

async fn retry(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let Some(id) = params.non_empty("value").and_then(parse_nzo_id) else {
        return Ok(error("expects one parameter"));
    };
    // Fangst resumes the failed job in place, so its id stays the same.
    Ok(match jobs.resume(id).await {
        Ok(job) => ok_json(&json!({ "status": true, "nzo_id": nzo_id(job.id) })),
        Err(_) => error("item does not exist"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_follow_sabnzbd() {
        assert_eq!(to_units(0, "B"), "0 B");
        assert_eq!(to_units(500, ""), "500");
        assert_eq!(to_units(2_048, "B"), "2 KB");
        assert_eq!(to_units(1_048_575, "B"), "1.0 MB");
        assert_eq!(to_units(1_288_490_189, "B"), "1.2 GB");
        assert_eq!(to_units(2_040_109_465, ""), "1.9 G");
    }

    #[test]
    fn ids_round_trip_and_reject_strangers() {
        assert_eq!(parse_nzo_id(&nzo_id(42)), Some(42));
        assert_eq!(parse_nzo_id("SABnzbd_nzo_p86tgx"), None);
        assert_eq!(parse_nzo_id("42"), None);
        assert_eq!(ids("SABnzbd_nzo_1,SABnzbd_nzo_x,SABnzbd_nzo_3"), [1, 3]);
    }

    #[test]
    fn mb_has_two_decimals() {
        assert_eq!(mb(1_339_755_315), "1277.69");
        assert_eq!(mb(0), "0.00");
    }

    #[test]
    fn time_left_is_hours_or_days() {
        assert_eq!(time_left(None), "0:00:00");
        assert_eq!(time_left(Some(0)), "0:00:00");
        assert_eq!(time_left(Some(1004)), "0:16:44");
        assert_eq!(time_left(Some(36_000 + 61)), "10:01:01");
        assert_eq!(time_left(Some(DAY + 3_661)), "1:01:01:01");
    }

    #[test]
    fn percentage_is_a_whole_number() {
        assert_eq!(percentage(0, 0), "0");
        assert_eq!(percentage(455, 1000), "45");
        assert_eq!(percentage(1000, 1000), "100");
        assert_eq!(percentage(2000, 1000), "100");
    }

    #[test]
    fn speed_keeps_sabnzbd_spacing() {
        assert_eq!(kb_per_sec(1_327_124), "1296.02");
        assert_eq!(speed(0), "0 ");
        assert_eq!(speed(1_327_124), "1.3 M");
    }

    fn running(total: i64, downloaded: i64, rate: i64, eta: Option<i64>) -> JobSummary {
        JobSummary {
            id: 1,
            kind: "usenet".to_owned(),
            name: "x".to_owned(),
            category: None,
            priority: 0,
            status: "downloading".to_owned(),
            error: None,
            output_path: None,
            total_bytes: total,
            downloaded_bytes: downloaded,
            rate_bytes_per_sec: rate,
            eta_seconds: eta,
            added_at: 0,
            finished_at: None,
        }
    }

    #[test]
    fn queue_slot_reports_progress() {
        let slot = queue_slot(0, &running(4 * 1_048_576, 1_048_576, 1_048_576, Some(3)));
        assert_eq!(slot["mb"], "4.00");
        assert_eq!(slot["mbleft"], "3.00");
        assert_eq!(slot["sizeleft"], "3.0 MB");
        assert_eq!(slot["percentage"], "25");
        assert_eq!(slot["timeleft"], "0:00:03");
    }

    #[test]
    fn header_sums_speed_and_time_left() {
        let jobs = [
            running(4 * 1_048_576, 1_048_576, 1_048_576, Some(3)),
            running(2 * 1_048_576, 0, 0, None),
        ];
        let header = header(&jobs);
        assert_eq!(header["kbpersec"], "1024.00");
        assert_eq!(header["speed"], "1.0 M");
        assert_eq!(header["mbleft"], "5.00");
        assert_eq!(header["timeleft"], "0:00:05");
    }
}
