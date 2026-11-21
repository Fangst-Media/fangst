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
  need a session variant. Done in #26, below.

## Amendment (#26): adapters with a login

qBittorrent clients sign in with a username and password and then send a `SID` cookie.

- **Two more credentials.** `Credential::Login { username, password }` is a sign-in request;
  `Credential::Session(token)` is a token from an earlier one. The daemon checks a login and hands
  the adapter a `LoginOutcome` (`Accepted { session, lifetime_secs }`, `Refused`, `RetryLater`),
  which `ApiAdapter::logged_in` turns into the protocol's answer (`Ok.` with `Set-Cookie: SID=...`,
  `Fails.`, or qBittorrent's 403 for a banned address).
- **Shared user store.** The password is checked like `POST /api/v1/auth/login`: the same users,
  the same Argon2 limits and the same per-address rate limit. One of the user's API keys is also
  accepted as the password, with any username, since Sonarr and Radarr offer only username and
  password fields and a fresh install has no password until one is set. The session is an
  ordinary Fangst session (30 days), so `SID` and `fangst_session` hold the same kind of token.
- **API keys directly.** `Authorization: Bearer <key>` and `X-API-Key: <key>` work as on
  qBittorrent 5.2 and later, without signing in.
- **Cross-site requests.** The adapter applies qBittorrent's own check: an `Origin`, or else a
  `Referer`, must name the host the request was sent to, or the answer is 401. Clients that send
  neither, such as Sonarr, pass. This is what keeps a cookie-carrying browser on another port of
  the same host from adding or deleting torrents, which the native API's `fangst_session` check
  covers there.

Alternatives: a separate adapter password in the config file (another secret to manage, and #31
isn't there yet); only API keys (Sonarr's API-key mode needs qBittorrent 5.2 detection, older
clients and HolyArrs's login path would not work); sessions scoped to the adapter (a schema change
for no gain, since the session already belongs to a user who may use the native API).

Consequences: a `SID` cookie works as `fangst_session` on the native API and the other way round.
`auth/logout` answers 200 but leaves the session until it expires, because adapters cannot end
sessions; `POST /api/v1/auth/logout` with the token ends it.
