//! The part of the SABnzbd API (`/api?mode=...&output=json`) that Sonarr, Radarr and `HolyArrs`
//! call, translated onto the Fangst job queue.
//!
//! The only state is the id mapping, and it is a formula: job 42 is `SABnzbd_nzo_42`. Queue
//! slots are the jobs that are queued, downloading or paused; history slots are completed and
//! failed jobs. `.claude/skills/arr-adapter-quirks/SKILL.md` lists what each client sends and
//! which fields it reads; `fixtures/adapters/sabnzbd/` holds the request and response pairs.

use std::sync::Arc;

use fangst_providers::{
    ApiAdapter, BoxFuture, Credential, HttpRequest, HttpResponse, Jobs, ProviderInfo, ProviderKind,
};

mod modes;
mod params;

pub use modes::{SAB_VERSION, nzo_id, parse_nzo_id};

/// The SABnzbd adapter; enable with `--enable adapter-sabnzbd`.
pub struct Sabnzbd;

pub const ID: &str = "adapter-sabnzbd";

/// SABnzbd answers `version` without a key, and `HolyArrs` relies on that to detect the server.
/// SABnzbd's `auth` mode is open too, but no client calls it, so it is not implemented.
const OPEN_MODES: [&str; 1] = ["version"];

impl ApiAdapter for Sabnzbd {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: ID.to_owned(),
            kind: ProviderKind::ApiAdapter,
            description: "SABnzbd API for Sonarr, Radarr and other SABnzbd clients".to_owned(),
        }
    }

    fn mount_path(&self) -> String {
        "/api".to_owned()
    }

    fn credential(&self, request: &HttpRequest) -> Credential {
        let params = params::parse(request);
        if params.get("mode").is_some_and(|m| OPEN_MODES.contains(&m)) {
            return Credential::NotNeeded;
        }
        params
            .non_empty("apikey")
            .map_or(Credential::Missing, |k| Credential::ApiKey(k.to_owned()))
    }

    /// SABnzbd 3.7 and later answer 403 with bare text, whatever `output` asks for. Sonarr then
    /// reports a generic 403 rather than "API key incorrect"; `HolyArrs` looks for `API Key`.
    fn rejected(&self, _request: &HttpRequest, missing: bool) -> HttpResponse {
        HttpResponse::text(
            403,
            if missing {
                "API Key Required"
            } else {
                "API Key Incorrect"
            },
        )
    }

    fn handle(&self, request: HttpRequest, jobs: Arc<dyn Jobs>) -> BoxFuture<HttpResponse> {
        Box::pin(async move {
            if request.path != "/" {
                return HttpResponse::text(404, "Not found");
            }
            let params = params::parse(&request);
            modes::dispatch(&params, jobs.as_ref()).await
        })
    }
}
