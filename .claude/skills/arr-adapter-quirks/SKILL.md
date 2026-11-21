---
name: arr-adapter-quirks
description: What Sonarr, Radarr, HolyArrs, Seerr and similar clients actually send to qBittorrent, SABnzbd and Sonarr/Radarr APIs, where it differs from the upstream docs. Read before changing any fangst-adapter-* crate.
---

# Adapter quirks

Clients depend on the behaviour of the real qBittorrent, SABnzbd, Sonarr and Radarr, not on their documentation. Every difference found goes here, with the client, the request, what the client expects and the recorded fixture that proves it.

Format for each entry:

```markdown
## Client: short description

**Request:** method, path and the parameters that matter.
**Expects:** the response shape or behaviour the client relies on.
**Fixture:** `fixtures/adapters/<adapter>/<file>`.
**Handled in:** `crates/fangst-adapter-<name>/src/<file>.rs`.
```

## Recorded from real clients

`tests/adapters` runs Sonarr 4.0.20.3014, Radarr 7.0.3.10918 and HolyArrs 1.2.0 against Fangst
(`just test-adapters`), and every request they send is kept in
`fixtures/adapters/<adapter>/recorded/<client>/`. The derived entries below were checked against
those recordings; where a client did something else, the entry here wins. The answers in the
recordings are Fangst's, which the clients accepted, not real SABnzbd or qBittorrent answers.

### HolyArrs: SABnzbd's history must not list torrent jobs

**Request:** `mode=history&start=0&limit=200` (no category on the first polls, later
`category=movies`), `last_history_update=<n>` once it has a counter.
**Expects:** only jobs it can map to its own grabs or that are really SABnzbd's. Fangst listed every
job, so a torrent HolyArrs had just sent to the qBittorrent adapter also showed up as a finished
SABnzbd job in its download list. The SABnzbd adapter now lists, pauses, deletes and retries only
usenet jobs, as the qBittorrent adapter already did for torrents.
**Fixture:** `sabnzbd/recorded/holyarrs/history.json`.
**Handled in:** `crates/fangst-adapter-sabnzbd/src/modes.rs` (`usenet_jobs`).

### HolyArrs: the complete folder must exist when the downloader is tested

**Request:** after `version`, `queue&limit=1` and `get_cats`, the SABnzbd test also sends
`mode=fullstatus&skip_dashboard=1` and `mode=get_config&section=categories`.
**Expects:** `status.completedir` and category `dir`s that exist on HolyArrs' side at that moment;
then it saves them as a path pair. A fresh Fangst had no download folder until the first job, so
HolyArrs saved no pair and parked the first usenet import under "needs a decision" ("HOLYARRS_SAVE_PATH
is not set and the downloader has no path pair"). The download service now creates its folder when
it starts.
**Fixture:** `sabnzbd/recorded/holyarrs/fullstatus.json`, `get_config.json`.
**Handled in:** `crates/fangst-downloads/src/service.rs` (`run`).

### HolyArrs 1.2.0: adds by URL, and adds torrents without state fields

**Request:** SABnzbd: `GET mode=addurl&name=<indexer link>&nzbname=<release>&cat=movies`, also when
the indexer link serves a plain NZB (no `addfile`), so Fangst fetches the NZB. qBittorrent:
`torrents/info?hashes=<hash>` first (a duplicate check, `[]` expected), `torrents/createCategory`,
then `torrents/add` as multipart with only `category` and `torrents` (filename `release.torrent`).
No `stopped`, `paused` or `savepath` when `HOLYARRS_SAVE_PATH` is unset; the derived
`add-stopped-holyarrs.json` describes the case with them. After import it sends
`mode=queue&name=delete&del_files=1` (Fangst answers `status: false`, the job is finished) and
then `mode=history&name=delete&del_files=1&archive=0`; it leaves torrents seeding.
**Fixture:** `sabnzbd/recorded/holyarrs/addurl.json`, `queue-delete.json`, `history-delete.json`,
`qbittorrent/recorded/holyarrs/torrents-info.json`, `torrents-add.json`.

### HolyArrs 1.2.0: qBittorrent sign-in

**Request:** `GET app/version` without a cookie first (not `app/webapiVersion`), which gets 403;
then `POST auth/login` with `password=...&username=...` (password first) and the version again.
`Referer: http://<host>:<port>` without a trailing slash on every call; user agent
`Go-http-client/1.1`. Polls `sync/maindata?rid=N` and `torrents/files?hash=`.
**Fixture:** `qbittorrent/recorded/holyarrs/app-version-403.json`, `auth-login.json`, `sync-maindata.json`.

### Sonarr and Radarr: Basic auth on every qBittorrent call

**Request:** `Authorization: Basic base64(username:password)` on every qBittorrent request,
including the first `app/webapiVersion` and `auth/login`, next to the `SID` cookie once signed in.
With Fangst the password is an API key, so it travels on every call; use HTTPS when the client
and Fangst are on different machines.
**Expects:** the Basic header is ignored: the request without a cookie still gets 403 `Forbidden`,
then sign-in works as described below.
**Fixture:** `qbittorrent/recorded/sonarr/app-webapiVersion-403.json`, `auth-login.json`.

### Sonarr and Radarr: what a grab and import sends

**Request:** SABnzbd: `addfile&cat=tv&priority=-100` (Radarr `cat=movies`) with one part
`name="name"; filename="<release>.nzb"`, `application/x-nzb`, as derived. After import with
"remove completed" on: `mode=history&name=delete&del_files=1&archive=1&value=<id>`.
qBittorrent: `torrents/add` multipart with `torrents` (`<release>.torrent`), `category`
(`tv-sonarr`, `radarr`) and `stopped=False`; no `paused`, no follow-ups, and no
`torrents/properties` polling with the default settings. After import `torrents/delete` with
`hashes=<hash>&deleteFiles=true`.
**Fixture:** `sabnzbd/recorded/sonarr/addfile.json`, `history-delete.json`,
`qbittorrent/recorded/sonarr/torrents-add.json`, `torrents-delete.json`; the same under `radarr/`.

### Media checks the clients run on import

Not adapter behaviour, but it decides whether an end-to-end import passes. Sonarr 4.0.20 and Radarr
7.0.3 refuse a file without an audio track ("No audio tracks detected") and treat a file shorter
than 10 minutes as a sample for an hour-long episode or a movie. HolyArrs 1.2.0 compares the
video's resolution with the release name ("file is 480p, name said 1080p") and, unless
`refuse_tiny_releases` is off, the downloaded size with the size the indexer claimed.

## SABnzbd

Read from Sonarr and Radarr `develop` (`src/NzbDrone.Core/Download/Clients/Sabnzbd/`), HolyArrs
`main` (`app/backend/internal/downloads/client_sabnzbd*.go`) and SABnzbd 4.5.x, 5.1.3 and develop
(`api.py`, `interface.py`, `misc.py`) in November 2026. Radarr sends the same requests as Sonarr.
The top-level fixtures are derived from these sources and SABnzbd's API docs; the client side is
also recorded (see "Recorded from real clients" above and `fixtures/adapters/sabnzbd/README.md`). Handled in `crates/fangst-adapter-sabnzbd/src/modes.rs`
unless named otherwise.

### All clients: where the key lives and what a failure looks like

**Request:** `apikey=` in the query (Sonarr, Radarr, HolyArrs). SABnzbd also reads it from a form
or multipart body. Sonarr sends `ma_username`/`ma_password` instead when its key setting is blank;
no current SABnzbd accepts those. HolyArrs calls `mode=version` without a key.
**Expects:** missing key: HTTP 403, plain text `API Key Required`; wrong key: 403,
`API Key Incorrect`; `output=json` does not change this (SABnzbd 3.7 and later). `version` works
without a key. Sonarr turns the 403 into a generic HTTP error; HolyArrs looks for `API Key` in
the body. An unknown mode, after the key check, is HTTP 200 `{"status": false, "error": "not implemented"}`.
**Fixture:** `apikey-missing.json`, `apikey-wrong.json`, `version-without-key.json`, `unknown-mode.json`.
**Handled in:** `crates/fangst-adapter-sabnzbd/src/lib.rs` (`credential`, `rejected`), `src/params.rs`.

### Sonarr: version check

**Request:** `GET /api?mode=version&apikey=K&output=json`.
**Expects:** `{"version": "..."}` matching `\d+\.\d+\.(\d+|x)` somewhere in the string, with
major >= 1 or minor >= 7. `4.x` fails; `develop` passes with a warning.
**Fixture:** `version.json`.

### Sonarr: connection test reads get_config

**Request:** `GET /api?mode=get_config&apikey=K&output=json` (no section, three times), and
`mode=fullstatus&skip_dashboard=1` when `misc.complete_dir` is not absolute.
**Expects:** `config.misc` present; `complete_dir` absolute (else `status.completedir` from
fullstatus); `pre_check` falsy; a category whose `name` equals Sonarr's category exactly, whose
`dir` does not end in `*`; `sorters` an array (Sonarr calls `.Any()` on it, so never null) with no
active sorter covering the category. Sonarr treats an empty `sort_cats` list as covering every
category, and `enable_tv_sorting: 1` with no `tv_categories` the same, so leave those keys out.
`history_retention_option: "all"` stops Sonarr deciding that SABnzbd deletes finished jobs.
**Fixture:** `get_config.json`, `fullstatus.json`.

### Sonarr: queue and history polling

**Request:** `mode=queue&start=0&limit=0&category=tv` and
`mode=history&start=0&limit=60&category=tv`. The filter parameter is `category`, not `cat`;
it is left out when the setting is blank. `limit=0` is "all" for the queue; for history SABnzbd
falls back to 10.
**Expects:** `queue.paused` (bool); per slot `nzo_id`, `filename`, `cat`, `priority` (a name:
Force, High, Normal, Low), `mb`, `mbleft`, `percentage` (an integer in a string; `"45.5"` fails),
`status`, `timeleft`. `timeleft` must be `H:MM:SS` or `D:HH:MM:SS`; anything else, including
empty, fails the whole queue parse. Slot `status` must be one of Sonarr's enum (Grabbing, Queued,
Paused, Checking, Downloading, QuickCheck, Verifying, Repairing, Fetching, Extracting, Moving,
Running, Completed, Failed, Deleted, Propagating); an unknown value fails the whole response.
History: `nzo_id`, `name`, `category`, `bytes` (number), `status`, `fail_message`, `storage`
(the finished folder; Sonarr walks up it to a folder named like `name`). Sonarr keeps only
items whose category equals its own exactly.
**Fixture:** `queue.json`, `history.json`.

### HolyArrs: strict JSON types and paging

**Request:** queue pages of `start=0,200,...&limit=200`, history with `last_history_update`,
filters `category` and `nzo_ids` (comma-separated).
**Expects:** Go's decoder fails the whole call on a type mismatch. Strings: `mb`, `mbleft`,
`percentage`, `timeleft`, `avg_age`, `kbpersec`, `diskspace1`, `diskspace2`, `speedlimit_abs`.
Numbers: `index`, `time_added`, `noofslots_total`, history `bytes`, `completed`, `time_added`.
Bool: `paused`. When `last_history_update` equals the current counter SABnzbd answers
`{"history": false}`; Fangst always answers with the full history and a counter of at least 1.
**Fixture:** `queue-page.json`, `history.json`.

### Sonarr and HolyArrs: adding an NZB

**Request:** Sonarr: `POST /api?mode=addfile&cat=tv&priority=-100&apikey=K&output=json`, one
multipart part `name="name"; filename="<release>.nzb"`, `Content-Type: application/x-nzb`; no
`nzbname`. Priority -100 is the category default, -2 adds paused. HolyArrs: the same with
`nzbname` and `application/octet-stream`, after checking the category with `get_cats`; or
`GET mode=addurl&name=<url>&cat=&nzbname=` when it has no file.
**Expects:** `{"status": true, "nzo_ids": ["..."]}`; both read `nzo_ids[0]` and fail on an empty
list. SABnzbd takes the upload from the `name` or `nzbfile` field and names the job after the
file unless `nzbname` is given.
**Fixture:** `addfile.json`, `addfile-nzbname-paused.json`, `addfile-no-file.json`, `addurl.json`, `get_cats.json`.

### Sonarr and HolyArrs: removing a job

**Request:** Sonarr reads the queue first, then sends `mode=queue&name=delete&del_files=0|1&value=<id>`
for a queued job, or `mode=history&name=delete&del_files=0|1&value=<id>&archive=0|1` for a
finished one (`archive=0` for failed). HolyArrs sends both, always with `archive=0`, and
tolerates `{"status": false}` on either.
**Expects:** queue delete `{"status": <removed>, "nzo_ids": [...]}`; history delete `{"status": true}`.
Fangst keeps no archive, so `archive=1` removes the job too.
**Fixture:** `queue-delete.json`, `history-delete.json`.

### HolyArrs: per-job pause, resume and retry

**Request:** `mode=queue&name=pause|resume&value=<id>`, `mode=retry&value=<id>`.
**Expects:** `{"status": bool, "nzo_ids": [...]}`; retry `{"status": true, "nzo_id": "<id>"}`.
SABnzbd gives a retried job a new id; Fangst resumes the job in place and returns the same id.
Not implemented (answer `not implemented`): global `mode=pause`/`resume`, `name=priority`,
`mode=switch`, `mode=config`, `mode=server_stats`. HolyArrs treats these as a refusal.
**Fixture:** `queue-pause.json`, `queue-resume.json`, `retry.json`.

## qBittorrent

Read from Sonarr and Radarr `develop` (`src/NzbDrone.Core/Download/Clients/QBittorrent/`), HolyArrs
`main` (`app/backend/internal/downloads/client_qbit*.go`) and qBittorrent `v4_6_x`, `v5_0_x` and
`master` (`src/webui/webapplication.cpp`, `api/authcontroller.cpp`, `api/torrentscontroller.cpp`,
`api/serialize/serialize_torrent.cpp`) in November 2026. Radarr sends the same requests as Sonarr,
without tags and without the inactive seeding limit. The top-level fixtures are derived; the client
side is also recorded (see "Recorded from real clients" above and
`fixtures/adapters/qbittorrent/README.md`). Handled in `crates/fangst-adapter-qbittorrent/src/api.rs`
unless named otherwise.

### All clients: signing in and what a failure looks like

**Request:** `POST /api/v2/auth/login`, form fields `username` and `password`. Sonarr sends it
url-encoded, keeps every `Set-Cookie` pair and sends no `Origin` or `Referer`; HolyArrs adds
`Referer: <base URL>` to every request and keeps cookies in a jar, so `Path` must be `/`. Sonarr's
API-key mode (qBittorrent 5.2 and later) sends `Authorization: Bearer <key>` and never signs in.
**Expects:** success: 200 `Ok.` with `Set-Cookie: SID=...`. Wrong password: **200** `Fails.`, not
401 or 403; HolyArrs reads 403 as a banned address and stops trying for 5 minutes. A request
without a valid session: 403 `Forbidden`; Sonarr then signs in once and retries. An `Origin`, or
else `Referer`, naming another host than `Host`: 401. Fangst takes a user's password, or any
username with one of the user's API keys as the password, and the `SID` is a Fangst session.
**Fixture:** `login.json`, `login-api-key.json`, `login-fails.json`, `expired-session.json`,
`cross-site.json`, `bearer-api-key.json`.
**Handled in:** `crates/fangst-adapter-qbittorrent/src/lib.rs`, `crates/fangst-dl-api/src/adapters.rs` (`login`).

### Sonarr: version probe before signing in

**Request:** `GET /api/v2/app/webapiVersion` with no cookie, cached for 10 minutes; then on every
poll again with the cookie.
**Expects:** 404 makes Sonarr fall back to the v1 API and 401 fails its test; 403 counts as v2.
The body is parsed with .NET `Version.Parse`: digits and dots only, no `v`. `app/version` has the
`v` (`v5.0.5`); HolyArrs only checks that it is short and does not start with `<` or `{`.
From 2.11.0 Sonarr sends `stopped` instead of `paused` on add; from 2.11.2 it reads `root_path`.
Fangst reports `v5.0.5` and `2.11.2`.
**Fixture:** `forbidden-without-session.json`, `app-version.json`, `app-webapiversion.json`.

### Sonarr: connection test reads preferences and categories

**Request:** `GET app/preferences`, `GET torrents/categories`, then `POST torrents/createCategory`
with `category` when its category is missing, and the list again.
**Expects:** the test fails when `max_ratio_enabled` (or `max_seeding_time_enabled` with under
20160 minutes) meets `max_ratio_act` 1 or 3 (remove), and when priority is "First" while
`queueing_enabled` is false. With `dht: false` Sonarr refuses magnet links without trackers.
`createCategory` must make the category appear in the next list; an exception from it is not
caught. Categories are `{name: {name, savePath}}`; an empty `savePath` makes Sonarr use the
preferences `save_path`. HolyArrs calls `createCategory` before every add and accepts 409 for an
existing one, which is what qBittorrent answers.
**Fixture:** `app-preferences.json`, `categories.json`, `create-category.json`, `create-category-exists.json`.

### Sonarr: adding a torrent

**Request:** `POST torrents/add` with `urls=<magnet>` or a multipart part `torrents` (filename
`<release>.torrent`, `application/octet-stream`), `category`, `stopped=False` or `True`
(capitalised: Sonarr formats booleans with .NET), optionally `sequentialDownload`,
`firstLastPiecePrio`, `contentLayout`, `ratioLimit`, `seedingTimeLimit`. No `savepath`. The whole
form becomes multipart when any value is over 1024 bytes, so a long magnet link arrives as
multipart too. HolyArrs sends both `stopped=true` and `paused=true`, plus `savepath`.
**Expects:** anything but the exact body `Fails.` is success; 409 is "release rejected". Sonarr
then polls `torrents/properties?hash=` up to ten times, 100 ms apart, until it answers 200, before
optional follow-ups (`setShareLimits`, `topPrio`, `setForceStart`, `addTags`) whose failures it
only logs. An uploaded file that is not a torrent is 415. Fangst ignores `savepath` (every job
has its own folder) and answers 404 to the follow-ups.
**Fixture:** `add-magnet.json`, `add-magnet-multipart.json`, `add-file.json`, `add-stopped-holyarrs.json`,
`add-fails.json`, `add-invalid-file.json`, `properties-unknown.json`, `unknown-endpoint.json`.

### Sonarr and HolyArrs: polling torrents

**Request:** Sonarr: `GET torrents/info?category=<cat>` (left out when blank). HolyArrs: `GET
sync/maindata?rid=N` for everything, `torrents/info?hashes=<a|b>` for one torrent.
**Expects:** Sonarr keys downloads by the hash in upper case and reads `hash`, `name`, `size`,
`progress`, `eta`, `state`, `category`, `save_path`, `content_path`, `root_path`, `ratio`,
`ratio_limit`, `seeding_time_limit`, `last_activity`. `eta` 8640000 means unknown. States:
`stoppedDL`/`pausedDL` paused; `queuedDL` and `checking*` queued; `metaDL` queued when `dht` is
on; `stoppedUP`, `uploading`, `stalledUP` and the other `*UP` states completed; `stalledDL`,
`missingFiles` and `error` a warning; anything else downloading. For a completed torrent Sonarr
imports `root_path` (2.11.2+) or `content_path`, and flags it when that equals `save_path`. It
removes a completed torrent after import only when the state is `stoppedUP` or `pausedUP` and its
ratio or seeding-time limit is reached. HolyArrs decodes strictly: `size`, `eta`, `added_on`,
`dlspeed` and the other counts must be JSON integers; a float such as `1.0` in an integer field
fails the whole poll. `sync/maindata` gives `torrents` as a map of hash to fields without `hash`;
`full_update: true` is always valid. An empty `category=` asks for torrents without a category.
Fangst maps queued to `queuedDL`, downloading to `downloading` (`metaDL` while a magnet link waits
for metadata), paused to `stoppedDL`, completed to `stoppedUP` and failed to `error`. `progress`,
`downloaded`, `amount_left`, `dlspeed` and `eta` come from the job's `downloaded_bytes`,
`rate_bytes_per_sec` and `eta_seconds` (8640000 when unknown). A finished
job has stopped seeding, so it reports `ratio_limit` 0 and `seeding_time_limit` 0, which lets
Sonarr and Radarr remove it after import when "remove completed" is on. Paths: a single-file
torrent's `save_path` is its job folder and `content_path` the file; a multi-file torrent's
`save_path` is the download root and `content_path` and `root_path` the job folder.
**Fixture:** `info-category.json`, `info-hashes.json`, `info-empty-category.json`, `maindata.json`,
`properties.json`, `files.json`.
**Handled in:** `crates/fangst-adapter-qbittorrent/src/torrent.rs`.

### Sonarr and HolyArrs: removing, pausing and recategorising

**Request:** `POST torrents/delete` with `hashes=<a|b>` and `deleteFiles=true`; Sonarr leaves
`deleteFiles` out when keeping the data, HolyArrs always sends it. `torrents/stop` and `start`
(HolyArrs falls back to `pause` and `resume` on 404). After import, when a post-import category is
set: `torrents/setCategory` with `hashes` and `category`.
**Expects:** 200 with an empty body. Real qBittorrent answers 400 to a delete without
`deleteFiles`; Fangst treats it as false, so Sonarr's "remove, keep files" works. Stopping a
finished torrent is not an error. `setCategory` to a category that does not exist is 409.
**Fixture:** `delete.json`, `delete-without-deletefiles.json`, `stop.json`, `start.json`,
`pause-legacy.json`, `resume-legacy.json`, `set-category.json`, `set-category-unknown.json`.
