# 0004. Adapters are mounted by the native API and authenticate with user API keys

Status: accepted
Date: 2026-11-21
Issue or PR: #22

## Context
The SABnzbd adapter is the first `ApiAdapter`. Each foreign protocol carries its credential in its
own place (`apikey=` in the query or a form field for SABnzbd, a login cookie for qBittorrent)
and reports a failed check in its own format, which clients parse. Something has to decide which
requests reach the adapter, where it lives in the URL space, and how SABnzbd's `addurl` fetches an
NZB without each adapter growing its own HTTP client.

## Decision
- **Mounting.** `fangst-dl-api` routes each enabled adapter's `mount_path` and everything below it
  (`router_with_adapters`). The native API owns `/api/v1/...` outright: a catch-all there answers
  404 for unknown native paths, so an adapter mounted at `/api` (SABnzbd) never sees them. A
  disabled adapter has no route at all, so its paths are 404.
- **Authentication.** `ApiAdapter` gains `credential()`, which reads what the request carries
  (`NotNeeded`, `ApiKey(key)` or `Missing`), and `rejected()`, which answers in the protocol's
  own error format. The daemon checks the key with `Auth::user_for_api_key`, the same keys as
  `X-Api-Key`. Session cookies never count for adapter routes, because SABnzbd changes state with
  plain GETs that any web page could trigger with the browser's cookie.
- **Job operations.** `fangst-dl-api` implements `Jobs` over `Service` (`ServiceJobs`). `Jobs`
  gains `add_nzb_url`, and the native API gains the same: `POST /api/v1/jobs` with
  `{"url": "..."}`.
- **Fetching by URL.** One `reqwest` client (rustls, already in the tree through librqbit, no
  system proxy) in `fangst-dl-api`. Only `http` and `https`, also for every redirect; at most 5
  redirects; 60 seconds for the whole request; bodies over 64 MiB (the NZB cap) are refused, both
  from `Content-Length` and while streaming. Errors name the status or limit, never the body.
  The limits are `FetchLimits`, constants until the defaults file is read.

## Alternatives
- **The daemon reads `apikey` itself.** Simpler trait, but SABnzbd puts the key in multipart bodies
  and qBittorrent uses a cookie; the daemon would have to know every protocol.
- **Mount SABnzbd at `/sabnzbd/api`.** Avoids sharing `/api` with the native API, but Sonarr and
  Radarr default to an empty URL base and call `/api`, so users would have to change a setting.
- **Blocking private and loopback addresses for URL fetches.** Indexers and Prowlarr usually sit
  on the same LAN or host as the daemon, so this breaks the main use. See below.
- **Fetching in `fangst-downloads`.** Keeps the service in charge, but adds an HTTP client to the
  service crate for what is input handling at the edge, next to multipart and JSON parsing.

## Consequences
- **SSRF.** Any holder of an API key can make the daemon send a GET to any http(s) address it can
  reach, including the LAN, loopback and cloud metadata endpoints. The response body is not
  returned; only whether it parsed as an NZB, the HTTP status, or the size limit. Key holders
  already control the daemon, so this widens their reach to the daemon's network, not their
  rights. Deployments that expose the daemon to untrusted key holders need egress filtering.
- A second adapter at `/api/...` (qBittorrent at `/api/v2`) works beside SABnzbd, since the more
  specific mount wins.
- Revisit when a protocol needs a login flow (qBittorrent's `/auth/login`): `credential()` may
  need a session variant.
