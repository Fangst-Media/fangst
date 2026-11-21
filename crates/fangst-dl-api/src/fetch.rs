//! Downloads an NZB or `.torrent` by URL, for `POST /api/v1/jobs` with a URL and for adapters.
//!
//! The URL comes from a caller holding an API key, so this is a server-side request on their
//! behalf: it can reach anything the daemon can, including addresses on the LAN and loopback.
//! That is on purpose, since indexers and Prowlarr usually run on the same network, and the
//! caller already controls the daemon. What stays fenced is the shape of the request: only
//! `http` and `https`, also after redirects, a bounded number of redirects, a time limit and a
//! size cap. Error messages name the status or the limit, never the remote body. A redirect to a
//! `magnet:` link is not followed but handed back, since torrent indexers answer that way for
//! releases they only know by magnet link.

use std::time::Duration;

use reqwest::redirect::{Attempt, Policy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchLimits {
    /// Bodies larger than this are refused, also when the server sends no length up front.
    pub max_bytes: usize,
    /// For the whole request, from connecting to the last byte.
    pub timeout: Duration,
    pub max_redirects: usize,
}

impl Default for FetchLimits {
    fn default() -> Self {
        Self {
            max_bytes: crate::MAX_NZB_BYTES,
            // Indexers generate NZBs on demand; a large one can take tens of seconds.
            timeout: Duration::from_mins(1),
            max_redirects: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FetchError {
    /// Not a URL, or not `http`/`https`.
    InvalidUrl(String),
    TooLarge(usize),
    /// The server answered, but not with 2xx.
    Status(u16),
    /// Connecting, TLS, a redirect the rules refuse, or the time limit.
    Network(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl(why) => write!(f, "invalid NZB URL: {why}"),
            Self::TooLarge(max) => write!(f, "the NZB at the URL is larger than {max} bytes"),
            Self::Status(status) => write!(f, "fetching the NZB failed with HTTP {status}"),
            Self::Network(why) => write!(f, "fetching the NZB failed: {why}"),
        }
    }
}

pub(crate) struct Fetched {
    pub bytes: Vec<u8>,
    /// From `Content-Disposition` or the last path segment, without `.nzb` or `.torrent`.
    pub file_name: Option<String>,
    /// Set instead of `bytes` when the URL redirected to a magnet link.
    pub magnet: Option<String>,
}

impl Fetched {
    /// A bencoded dictionary starts with `d`; an NZB is XML. Parsing decides the rest.
    pub(crate) fn is_torrent(&self) -> bool {
        self.magnet.is_some() || self.bytes.first() == Some(&b'd')
    }
}

#[derive(Clone)]
pub(crate) struct Fetcher {
    client: Result<reqwest::Client, String>,
    limits: FetchLimits,
}

fn allowed_scheme(url: &reqwest::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

impl Fetcher {
    pub(crate) fn new(limits: FetchLimits) -> Self {
        let max_redirects = limits.max_redirects;
        let policy = Policy::custom(move |attempt: Attempt| {
            if attempt.previous().len() > max_redirects {
                attempt.error("too many redirects")
            } else if attempt.url().scheme() == "magnet" {
                attempt.stop()
            } else if !allowed_scheme(attempt.url()) {
                attempt.error("redirect to a URL that is not http or https")
            } else {
                attempt.follow()
            }
        });
        let client = reqwest::Client::builder()
            .redirect(policy)
            .timeout(limits.timeout)
            .user_agent(concat!("fangst/", env!("CARGO_PKG_VERSION")))
            .build()
            // Only fails when the TLS backend can't start; every fetch then reports it.
            .map_err(|e| e.to_string());
        Self { client, limits }
    }

    pub(crate) async fn fetch(&self, url: &str) -> Result<Fetched, FetchError> {
        let parsed = reqwest::Url::parse(url).map_err(|e| FetchError::InvalidUrl(e.to_string()))?;
        if !allowed_scheme(&parsed) {
            return Err(FetchError::InvalidUrl(
                "only http and https URLs are fetched".to_owned(),
            ));
        }
        let client = self
            .client
            .as_ref()
            .map_err(|e| FetchError::Network(e.clone()))?;
        let mut response = client
            .get(parsed)
            .send()
            .await
            .map_err(|e| FetchError::Network(describe(&e)))?;
        // reqwest hands back a redirect it won't follow, such as one to `file:`, unfollowed.
        if response.status().is_redirection() {
            if let Some(link) = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .filter(|v| v.starts_with("magnet:"))
            {
                return Ok(Fetched {
                    bytes: Vec::new(),
                    file_name: None,
                    magnet: Some(link.to_owned()),
                });
            }
            return Err(FetchError::Network(
                "redirect to a URL that is not http or https".to_owned(),
            ));
        }
        if !response.status().is_success() {
            return Err(FetchError::Status(response.status().as_u16()));
        }
        let max = self.limits.max_bytes;
        if response
            .content_length()
            .is_some_and(|length| length > max as u64)
        {
            return Err(FetchError::TooLarge(max));
        }
        let file_name = disposition_name(response.headers())
            .or_else(|| path_name(response.url()))
            .map(|name| strip_extension(&name));
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| FetchError::Network(describe(&e)))?
        {
            if bytes.len() + chunk.len() > max {
                return Err(FetchError::TooLarge(max));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(Fetched {
            bytes,
            file_name,
            magnet: None,
        })
    }
}

/// reqwest's message for a refused redirect is only in the error's source chain.
fn describe(e: &reqwest::Error) -> String {
    let mut message = if e.is_timeout() {
        "timed out".to_owned()
    } else {
        e.to_string()
    };
    let mut source = std::error::Error::source(e);
    while let Some(inner) = source {
        message.push_str(": ");
        message.push_str(&inner.to_string());
        source = inner.source();
    }
    message
}

fn disposition_name(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let value = headers
        .get(reqwest::header::CONTENT_DISPOSITION)?
        .to_str()
        .ok()?;
    let (_, rest) = value.split_once("filename=")?;
    let name = rest.split(';').next()?.trim().trim_matches('"').trim();
    clean(name)
}

fn path_name(url: &reqwest::Url) -> Option<String> {
    clean(url.path_segments()?.next_back()?)
}

/// The name only labels the job; folder names are made safe where the job folder is created.
fn clean(name: &str) -> Option<String> {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn strip_extension(name: &str) -> String {
    for extension in [".nzb", ".torrent"] {
        if let Some((stem, ext)) = name
            .len()
            .checked_sub(extension.len())
            .and_then(|cut| name.split_at_checked(cut))
            && !stem.is_empty()
            && ext.eq_ignore_ascii_case(extension)
        {
            return stem.to_owned();
        }
    }
    name.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_from_the_header_then_the_path() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_DISPOSITION,
            "attachment; filename=\"Show.S01E01.nzb\"".parse().unwrap(),
        );
        assert_eq!(
            disposition_name(&headers)
                .map(|n| strip_extension(&n))
                .as_deref(),
            Some("Show.S01E01")
        );
        let url = reqwest::Url::parse("http://x/get/Movie.2020.NZB").unwrap();
        assert_eq!(
            path_name(&url).map(|n| strip_extension(&n)).as_deref(),
            Some("Movie.2020")
        );
        let url = reqwest::Url::parse("http://x/dl/Some.Release.Torrent").unwrap();
        assert_eq!(
            path_name(&url).map(|n| strip_extension(&n)).as_deref(),
            Some("Some.Release")
        );
        let url = reqwest::Url::parse("http://x/").unwrap();
        assert_eq!(path_name(&url), None);
    }
}
