//! The part of the qBittorrent web API v2 (`/api/v2/...`) that Sonarr, Radarr and `HolyArrs`
//! call, translated onto the Fangst job queue.
//!
//! Torrent jobs appear as torrents keyed by their info hash; usenet jobs are not listed. The
//! adapter keeps no state: hashes come from the jobs, sessions from the daemon's user store.
//! `.claude/skills/arr-adapter-quirks/SKILL.md` lists what each client sends and which fields it
//! reads; `fixtures/adapters/qbittorrent/` holds the request and response pairs.
//!
//! Sign-in: `auth/login` takes a Fangst username and password, or any username with one of the
//! user's API keys as the password, and sets the `SID` cookie to a Fangst session. API keys also
//! work directly as `Authorization: Bearer` or `X-API-Key`, as newer qBittorrent releases accept.

// `json!` with the 60-odd fields of a qBittorrent torrent needs more than the default 128.
#![recursion_limit = "256"]

use std::sync::Arc;

use fangst_providers::{
    ApiAdapter, BoxFuture, Credential, HttpRequest, HttpResponse, Jobs, LoginOutcome, ProviderInfo,
    ProviderKind,
};

mod api;
mod params;
mod torrent;

pub use api::{API_VERSION, APP_VERSION};

/// The qBittorrent adapter; enable with `--enable adapter-qbittorrent`.
pub struct Qbittorrent;

pub const ID: &str = "adapter-qbittorrent";

const SESSION_COOKIE: &str = "SID";

impl ApiAdapter for Qbittorrent {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: ID.to_owned(),
            kind: ProviderKind::ApiAdapter,
            description: "qBittorrent WebUI API for Sonarr, Radarr and other qBittorrent clients"
                .to_owned(),
        }
    }

    fn mount_path(&self) -> String {
        "/api/v2".to_owned()
    }

    fn credential(&self, request: &HttpRequest) -> Credential {
        // qBittorrent refuses a request from another site before looking at its credentials;
        // `rejected` turns this into the 401 it answers with.
        if cross_site(request) {
            return Credential::Missing;
        }
        if request.path == "/auth/login" {
            let params = params::parse(request);
            return Credential::Login {
                username: params.get("username").unwrap_or_default().to_owned(),
                password: params.get("password").unwrap_or_default().to_owned(),
            };
        }
        if let Some(key) = api_key(request) {
            return Credential::ApiKey(key);
        }
        session(request).map_or(Credential::Missing, Credential::Session)
    }

    /// qBittorrent answers 401 to a request from another site and 403 `Forbidden` to one without
    /// a valid session. Sonarr re-logs in once on 403; `HolyArrs` signs in on 401 or 403.
    fn rejected(&self, request: &HttpRequest, _missing: bool) -> HttpResponse {
        if cross_site(request) {
            HttpResponse::text(401, "Unauthorized")
        } else {
            HttpResponse::text(403, "Forbidden")
        }
    }

    /// A wrong password is HTTP 200 with `Fails.`; `HolyArrs` reads a 403 here as a banned
    /// address, which is what qBittorrent sends after too many failures.
    fn logged_in(&self, _request: &HttpRequest, outcome: LoginOutcome) -> HttpResponse {
        match outcome {
            LoginOutcome::Accepted {
                session,
                lifetime_secs,
            } => {
                let mut response = HttpResponse::text(200, "Ok.");
                response.headers.push((
                    "set-cookie".to_owned(),
                    format!(
                        "{SESSION_COOKIE}={session}; HttpOnly; Path=/; SameSite=Strict; Max-Age={lifetime_secs}"
                    ),
                ));
                response
            }
            LoginOutcome::Refused => HttpResponse::text(200, "Fails."),
            LoginOutcome::RetryLater { seconds } => {
                let mut response = HttpResponse::text(
                    403,
                    "Your IP address has been banned after too many failed authentication attempts.",
                );
                response
                    .headers
                    .push(("retry-after".to_owned(), seconds.to_string()));
                response
            }
        }
    }

    fn handle(&self, request: HttpRequest, jobs: Arc<dyn Jobs>) -> BoxFuture<HttpResponse> {
        Box::pin(async move { api::dispatch(&request, jobs.as_ref()).await })
    }
}

fn api_key(request: &HttpRequest) -> Option<String> {
    let bearer = request.header("authorization").and_then(|value| {
        let (scheme, key) = value.trim().split_once(' ')?;
        scheme.eq_ignore_ascii_case("bearer").then(|| key.trim())
    });
    bearer
        .or_else(|| request.header("x-api-key").map(str::trim))
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
}

fn session(request: &HttpRequest) -> Option<String> {
    request
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("cookie"))
        .flat_map(|(_, value)| value.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == SESSION_COOKIE && !value.is_empty()).then(|| value.to_owned())
        })
}

/// qBittorrent's CSRF rule: with an `Origin`, or else a `Referer`, its host and port must equal
/// the `Host` the request was sent to. Requests with neither, as Sonarr sends them, pass. A
/// cookie set by this adapter would otherwise let any page on another port of the same host
/// add and delete torrents.
fn cross_site(request: &HttpRequest) -> bool {
    let Some(target) = request
        .header("x-forwarded-host")
        .or_else(|| request.header("host"))
    else {
        return false;
    };
    let source = request
        .header("origin")
        .filter(|o| !o.trim().is_empty() && *o != "null")
        .or_else(|| request.header("referer"));
    let Some(source) = source else {
        return false;
    };
    let authority = source
        .split_once("://")
        .map_or(source, |(_, rest)| rest)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    !authority.eq_ignore_ascii_case(target.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(headers: &[(&str, &str)]) -> HttpRequest {
        HttpRequest {
            method: "GET".to_owned(),
            path: "/app/version".to_owned(),
            query: Vec::new(),
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            body: Vec::new(),
        }
    }

    #[test]
    fn origin_or_referer_must_name_the_host() {
        let host = ("host", "nas:7740");
        assert!(!cross_site(&request(&[host])));
        assert!(!cross_site(&request(&[
            host,
            ("referer", "http://nas:7740/")
        ])));
        assert!(!cross_site(&request(&[
            host,
            ("origin", "http://NAS:7740")
        ])));
        assert!(cross_site(&request(&[host, ("origin", "http://nas:8080")])));
        assert!(cross_site(&request(&[
            host,
            ("referer", "https://evil.example/x?nas:7740")
        ])));
        assert!(!cross_site(&request(&[
            ("host", "127.0.0.1:7740"),
            ("x-forwarded-host", "dl.example.com"),
            ("origin", "https://dl.example.com")
        ])));
    }

    #[test]
    fn keys_come_from_bearer_or_x_api_key_and_sessions_from_sid() {
        assert_eq!(
            api_key(&request(&[("authorization", "Bearer k1")])).as_deref(),
            Some("k1")
        );
        assert_eq!(
            api_key(&request(&[("x-api-key", "k2")])).as_deref(),
            Some("k2")
        );
        assert_eq!(api_key(&request(&[("authorization", "Basic YTpi")])), None);
        assert_eq!(
            session(&request(&[("cookie", "other=1; SID=abc")])).as_deref(),
            Some("abc")
        );
        assert_eq!(session(&request(&[("cookie", "SID=")])), None);
    }
}
