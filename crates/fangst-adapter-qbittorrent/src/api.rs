//! The endpoints, each translated onto [`Jobs`]. Paths are relative to `/api/v2`.

use std::path::Path;

use fangst_providers::{AddTorrent, HttpRequest, HttpResponse, Jobs, ProviderError, TorrentInput};
use serde_json::{Map, Value, json};

use crate::params::{self, Hashes, Params};
use crate::torrent::Torrent;

/// The qBittorrent release reported to clients, paired with its web API version. API 2.11
/// renamed pause and resume to stop and start and the `paused` add option to `stopped`; Sonarr
/// and Radarr pick their requests by this number, and use `root_path` from 2.11.2.
pub const APP_VERSION: &str = "v5.0.5";
pub const API_VERSION: &str = "2.11.2";

pub(crate) async fn dispatch(request: &HttpRequest, jobs: &dyn Jobs) -> HttpResponse {
    let params = params::parse(request);
    let result = match request.path.trim_end_matches('/') {
        "/app/version" => Ok(HttpResponse::text(200, APP_VERSION)),
        "/app/webapiVersion" => Ok(HttpResponse::text(200, API_VERSION)),
        "/app/preferences" => Ok(preferences(jobs)),
        "/app/defaultSavePath" => Ok(HttpResponse::text(200, root_text(jobs))),
        // Sessions end when they expire; the daemon's own logout ends them sooner.
        "/auth/logout" => Ok(HttpResponse::text(200, "")),
        "/torrents/info" => info(&params, jobs).await,
        "/torrents/properties" => one(&params, jobs, Torrent::properties).await,
        "/torrents/files" => one(&params, jobs, Torrent::files).await,
        "/torrents/add" => add(&params, jobs).await,
        "/torrents/delete" => delete(&params, jobs).await,
        "/torrents/stop" | "/torrents/pause" => each(&params, jobs, Action::Pause).await,
        "/torrents/start" | "/torrents/resume" => each(&params, jobs, Action::Resume).await,
        "/torrents/categories" => categories(jobs).await,
        "/torrents/createCategory" => create_category(&params, jobs).await,
        "/torrents/setCategory" => set_category(&params, jobs).await,
        "/sync/maindata" => maindata(&params, jobs).await,
        _ => Ok(HttpResponse::text(404, "Not Found")),
    };
    result.unwrap_or_else(|e| {
        let status = if e.retryable { 500 } else { 400 };
        HttpResponse::text(status, e.message)
    })
}

fn root_text(jobs: &dyn Jobs) -> String {
    jobs.download_root().to_string_lossy().into_owned()
}

/// Every torrent job with its torrent status, newest first. Usenet jobs are left out.
async fn torrents(jobs: &dyn Jobs) -> Result<Vec<Torrent>, ProviderError> {
    let mut out = Vec::new();
    for job in jobs.list().await? {
        if job.kind != "torrent" {
            continue;
        }
        // A job deleted between the two reads has no status; leave it out.
        if let Some(status) = jobs.torrent(job.id).await? {
            out.push(Torrent { job, status });
        }
    }
    Ok(out)
}

/// The fields Sonarr, Radarr and `HolyArrs` read. No share limits, so clients never warn that
/// torrents are removed at a ratio; queueing on, since Fangst does queue torrents beyond its
/// limit; DHT on, so Sonarr accepts magnet links without trackers.
fn preferences(jobs: &dyn Jobs) -> HttpResponse {
    HttpResponse::json(
        200,
        &json!({
            "save_path": root_text(jobs),
            "temp_path_enabled": false,
            "temp_path": "",
            "max_ratio_enabled": false,
            "max_ratio": -1,
            "max_seeding_time_enabled": false,
            "max_seeding_time": -1,
            "max_inactive_seeding_time_enabled": false,
            "max_inactive_seeding_time": -1,
            "max_ratio_act": 0,
            "queueing_enabled": true,
            "dht": true,
            "auto_tmm_enabled": false,
            "torrent_content_layout": "Original",
            "add_stopped_enabled": false,
            "web_ui_csrf_protection_enabled": true,
            "bypass_local_auth": false,
            "bypass_auth_subnet_whitelist_enabled": false,
        }),
    )
}

async fn info(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let root = jobs.download_root();
    let hashes = params.hashes();
    // `category=` (empty) asks for torrents without a category, unlike leaving it out.
    let category = params.get("category");
    let items: Vec<Value> = torrents(jobs)
        .await?
        .iter()
        .filter(|t| hashes.as_ref().is_none_or(|h| h.contains(t.hash())))
        .filter(|t| category.is_none_or(|c| t.job.category.as_deref().unwrap_or_default() == c))
        .map(|t| t.info(&root))
        .collect();
    Ok(HttpResponse::json(200, &items))
}

async fn find(params: &Params, jobs: &dyn Jobs) -> Result<Option<Torrent>, ProviderError> {
    let Some(Hashes::These(hashes)) = params.hashes() else {
        return Ok(None);
    };
    let Some(hash) = hashes.first() else {
        return Ok(None);
    };
    Ok(torrents(jobs).await?.into_iter().find(|t| t.hash() == hash))
}

/// `properties` and `files` for `hash=`; an unknown hash is 404, which Sonarr waits out after
/// adding a torrent.
async fn one(
    params: &Params,
    jobs: &dyn Jobs,
    view: fn(&Torrent, &Path) -> Value,
) -> Result<HttpResponse, ProviderError> {
    Ok(match find(params, jobs).await? {
        Some(torrent) => HttpResponse::json(200, &view(&torrent, &jobs.download_root())),
        None => HttpResponse::text(404, "Not Found"),
    })
}

/// `urls` (one per line: magnet links or http(s) URLs) and `torrents` file parts. `Ok.` when at
/// least one went in, `Fails.` when none did, as qBittorrent 5.0 answers. `savepath` is ignored:
/// every job gets its own folder under the download root. `stopped` (5.x) and `paused` (4.x)
/// both add the torrent paused; `rename` names the job.
async fn add(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let mut inputs: Vec<TorrentInput> = params
        .get("urls")
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(|u| {
            if u.starts_with("magnet:") {
                TorrentInput::Magnet(u.to_owned())
            } else {
                TorrentInput::Url(u.to_owned())
            }
        })
        .collect();
    let files: Vec<&params::Upload> = params
        .uploads
        .iter()
        .filter(|u| u.field == "torrents")
        .collect();
    // qBittorrent checks uploaded files before adding anything and refuses the request with
    // 415 when one isn't a torrent; a bencoded file starts with a dictionary.
    if files.iter().any(|f| f.bytes.first() != Some(&b'd')) {
        return Ok(HttpResponse::text(415, "Torrent file is not valid."));
    }
    inputs.extend(files.iter().map(|f| TorrentInput::File(f.bytes.clone())));
    if inputs.is_empty() {
        return Ok(HttpResponse::text(200, "Fails."));
    }
    let category = params.non_empty("category").map(str::to_owned);
    let stopped = params
        .flag("stopped")
        .or_else(|| params.flag("paused"))
        .unwrap_or(false);
    let name = params.non_empty("rename").map(str::to_owned);
    let mut added = 0;
    for input in inputs {
        let request = AddTorrent {
            input,
            name: name.clone(),
            category: category.clone(),
            priority: 0,
        };
        let Ok(job) = jobs.add_torrent(request).await else {
            continue;
        };
        added += 1;
        if stopped {
            // Fangst adds jobs to the queue; stopping right after keeps it from starting.
            let _ = jobs.pause(job.id).await;
        }
    }
    Ok(HttpResponse::text(
        200,
        if added > 0 { "Ok." } else { "Fails." },
    ))
}

/// `deleteFiles` is required by qBittorrent, but Sonarr leaves it out when keeping the data, so
/// a missing value means keep. Unknown hashes are skipped silently.
async fn delete(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let Some(hashes) = params.hashes() else {
        return Ok(HttpResponse::text(400, "Bad Request"));
    };
    let delete_files = params.flag("deleteFiles").unwrap_or(false);
    for torrent in torrents(jobs).await? {
        if hashes.contains(torrent.hash()) {
            jobs.delete(torrent.job.id, delete_files).await?;
        }
    }
    Ok(HttpResponse::text(200, ""))
}

#[derive(Clone, Copy)]
enum Action {
    Pause,
    Resume,
}

/// Stopping a finished torrent or starting a running one is not an error in qBittorrent, so
/// refusals from the queue for those are ignored.
async fn each(
    params: &Params,
    jobs: &dyn Jobs,
    action: Action,
) -> Result<HttpResponse, ProviderError> {
    let Some(hashes) = params.hashes() else {
        return Ok(HttpResponse::text(400, "Bad Request"));
    };
    for torrent in torrents(jobs).await? {
        if !hashes.contains(torrent.hash()) {
            continue;
        }
        let result = match action {
            Action::Pause => jobs.pause(torrent.job.id).await,
            Action::Resume => jobs.resume(torrent.job.id).await,
        };
        match result {
            Err(e) if e.retryable => return Err(e),
            _ => {}
        }
    }
    Ok(HttpResponse::text(200, ""))
}

/// Categories have no save path of their own; clients then use `save_path` from the
/// preferences, the download root.
async fn categories(jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let map: Map<String, Value> = jobs
        .categories()
        .await?
        .into_iter()
        .map(|name| {
            let entry = json!({ "name": name, "savePath": "" });
            (name, entry)
        })
        .collect();
    Ok(HttpResponse::json(200, &Value::Object(map)))
}

/// 409 for a category that exists, as qBittorrent answers; `HolyArrs` creates before every add
/// and expects it. `savePath` is ignored like `savepath` on add.
async fn create_category(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let Some(name) = params.non_empty("category").map(str::trim) else {
        return Ok(HttpResponse::text(400, "Category name cannot be empty"));
    };
    if jobs.categories().await?.iter().any(|c| c == name) {
        return Ok(HttpResponse::text(409, "Unable to create category"));
    }
    match jobs.create_category(name.to_owned()).await {
        Ok(()) => Ok(HttpResponse::text(200, "")),
        Err(e) if !e.retryable => Ok(HttpResponse::text(409, "Unable to create category")),
        Err(e) => Err(e),
    }
}

/// An empty `category` removes it; an unknown one is 409, as qBittorrent answers.
async fn set_category(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let Some(hashes) = params.hashes() else {
        return Ok(HttpResponse::text(400, "Bad Request"));
    };
    let category = params.non_empty("category").map(|c| c.trim().to_owned());
    if let Some(name) = &category
        && !jobs.categories().await?.contains(name)
    {
        return Ok(HttpResponse::text(409, "Incorrect category name"));
    }
    for torrent in torrents(jobs).await? {
        if hashes.contains(torrent.hash()) {
            jobs.set_category(torrent.job.id, category.clone()).await?;
        }
    }
    Ok(HttpResponse::text(200, ""))
}

/// `HolyArrs` polls this instead of `torrents/info`. Every answer is a full update: nothing is
/// kept between polls to compute a difference from, and a full update is always valid.
async fn maindata(params: &Params, jobs: &dyn Jobs) -> Result<HttpResponse, ProviderError> {
    let root = jobs.download_root();
    let rid = params
        .get("rid")
        .and_then(|r| r.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let all = torrents(jobs).await?;
    let (mut down, mut up, mut have, mut sent) = (0u64, 0u64, 0u64, 0u64);
    let mut items = Map::new();
    for torrent in &all {
        down += torrent.download_bps();
        up += torrent.status.upload_bps;
        have += torrent.have();
        sent += torrent.status.uploaded_bytes;
        let mut info = torrent.info(&root);
        if let Some(object) = info.as_object_mut() {
            object.remove("hash");
        }
        items.insert(torrent.hash().to_owned(), info);
    }
    let categories: Map<String, Value> = jobs
        .categories()
        .await?
        .into_iter()
        .map(|name| {
            let entry = json!({ "name": name, "savePath": "" });
            (name, entry)
        })
        .collect();
    Ok(HttpResponse::json(
        200,
        &json!({
            "rid": rid + 1,
            "full_update": true,
            "torrents": items,
            "torrents_removed": [],
            "categories": categories,
            "categories_removed": [],
            "tags": [],
            "tags_removed": [],
            "server_state": {
                "alltime_dl": have,
                "alltime_ul": sent,
                "dl_info_data": have,
                "up_info_data": sent,
                "dl_info_speed": down,
                "up_info_speed": up,
                "dl_rate_limit": 0,
                "up_rate_limit": 0,
                "free_space_on_disk": 0,
                "connection_status": "connected",
                "use_alt_speed_limits": false,
                "queueing": true,
            },
        }),
    ))
}
