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

## SABnzbd

Read from Sonarr and Radarr `develop` (`src/NzbDrone.Core/Download/Clients/Sabnzbd/`), HolyArrs
`main` (`app/backend/internal/downloads/client_sabnzbd*.go`) and SABnzbd 4.5.x, 5.1.3 and develop
(`api.py`, `interface.py`, `misc.py`) in November 2026. Radarr sends the same requests as Sonarr.
Every fixture is derived from these sources and SABnzbd's API docs, not recorded; see
`fixtures/adapters/sabnzbd/README.md`. Handled in `crates/fangst-adapter-sabnzbd/src/modes.rs`
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
