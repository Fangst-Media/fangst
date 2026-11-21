# qBittorrent adapter fixtures

One request and the response real qBittorrent gives it, per file. `crates/fangst-adapter-qbittorrent/tests/fixtures.rs`
sends every request through the adapter, with the sign-in the daemon does around it, backed by the
fixed jobs listed below, and checks:

- the HTTP status, and the content type (JSON or plain text);
- plain-text bodies match exactly;
- every key in the fixture's JSON exists in the adapter's answer with the same JSON type
  (string, integer, float, bool, null, array, object), recursively. Array items, and the values of
  `sync/maindata`'s `torrents` map, are checked against the fixture's first one. Integers and
  floats count as different types, since Go clients refuse `1.0` where they expect an integer. The
  adapter may add keys; it may not drop or retype one;
- each JSON pointer in `exact` has the same value in both, with `\` in paths read as `/`. A
  pointer past the end of an array checks that the answer has no such item;
- `response.headers`: each named header starts with the given text;
- `added` (job 7 as added), `gone`, `statuses`, `job_categories` and `categories` (the created
  ones, afterwards) describe what the request changed. A fixture without `added` adds nothing.

## Provenance

`"source": "derived"` on every file. None of these was recorded from a running qBittorrent: no
container was available when they were written. Each was built from:

- qBittorrent's source on the `v4_6_x`, `v5_0_x` and `master` branches (`webapplication.cpp` for
  the 403 `Forbidden` and 401 CSRF answers and the `SID` cookie, `authcontroller.cpp` for `Ok.` and
  `Fails.`, `torrentscontroller.cpp` for add, delete, category and 409/415 answers,
  `serialize_torrent.cpp` for the torrent fields and their types) and the WebUI API wiki page;
- the request each client sends, read from the qBittorrent clients in Sonarr and Radarr
  (`develop`) and HolyArrs (`main`). `client` names which one sends the request in this form;
  `qbittorrent` marks a case taken from qBittorrent's behaviour that no client was seen sending.

Values in the bodies describe the jobs below where `exact` checks them; elsewhere they are
plausible qBittorrent values and only their types count. Replace a file with a recorded one when a
real qBittorrent is captured; keep the `exact` pointers that still hold.

## Recorded from clients

`recorded/<client>/` holds what Sonarr, Radarr and HolyArrs actually sent to Fangst in the
end-to-end test (`tests/adapters`), one file per distinct request, with Fangst's answer, which the
client accepted: the run imported its releases. `"source": "recorded"`, `client` and
`client_version` say who sent it. These record the requests as real clients send them; the
answers are Fangst's, not qBittorrent's. Binary bodies (torrent uploads) are in `body_hex`.
`just test-adapters` then `just record-adapter-fixtures` writes them again.

`recorded_client_requests_are_understood` in the fixture tests replays each request against the
jobs below, signed in with the placeholder session. Answers that depend on which jobs exist
(`"job_specific": true`) only have to be understood (no 400, 401, 403, 415 or 5xx unless the
recorded answer was one); the others must match in status, JSON shape and plain-text body.

## Jobs behind the fixtures

| Fangst id | Kind | Name | Category | Status | Bytes | Info hash | Files |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | torrent | `Show.S01E01.1080p.WEB-DL` | tv-sonarr | downloading, half done at 2 MiB/s | 1468006400 | `1` x 40 | one `.mkv` |
| 2 | torrent | `Show.S01E02.1080p.WEB-DL` | tv-sonarr | paused | 1468006400 | `2` x 40 | one `.mkv` |
| 3 | torrent | `Movie.2020.1080p.BluRay` | radarr | queued magnet link, no metadata | 0 | `3` x 40 | none yet |
| 4 | torrent | `Show.S01E03.1080p.WEB-DL` | tv-sonarr | completed | 1468007424 | `4` x 40 | `.mkv` and `.nfo` |
| 5 | torrent | `Show.S01E04.1080p.WEB-DL` | tv-sonarr | failed | 1468006400 | `5` x 40 | one `.mkv` |
| 6 | usenet | `Usenet.Job` | tv-sonarr | queued | 1000 | | never listed |

The download root is `/data/downloads`; jobs 1, 2, 4 and 5 have the folder `/data/downloads/<id>-<name>`.
Created categories are `radarr` and `tv-sonarr`. A `.torrent` upload becomes info hash `7` x 40.
The API key is `fixture-api-key-0000`, user `admin` has the password `fixture-password-0000`, and
a successful sign-in gets the session `fixture-session-0000`; all placeholders.
