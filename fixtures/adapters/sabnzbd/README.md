# SABnzbd adapter fixtures

One request and the response real SABnzbd gives it, per file. `crates/fangst-adapter-sabnzbd/tests/fixtures.rs`
sends every request through the adapter, backed by the fixed jobs listed below, and checks:

- the HTTP status and the content type (JSON or plain text) match;
- plain-text bodies match exactly;
- every key in the fixture's JSON exists in the adapter's answer with the same JSON type
  (string, number, bool, null, array, object), recursively, with array items checked against the
  fixture's first item. The adapter may add keys; it may not drop or retype one;
- each JSON pointer in `exact` has the same value in both.

## Provenance

`"source": "derived"` on every file. None of these was recorded from a running SABnzbd: no
SABnzbd container was available when they were written. Each was built from:

- SABnzbd 4.5 API documentation examples (field names, types, formats such as `"mb": "1277.65"`,
  `"size": "1.2 GB"`, `"timeleft": "0:16:44"`);
- SABnzbd's `api.py`, `interface.py` and `misc.py` (5.1.3 and develop) for what the docs leave
  out: the 403 text bodies for a missing or wrong key, `{"status": false, "error": "not implemented"}`
  for unknown modes, `nzo_ids` on add, and `to_units` formatting;
- the request each client sends, read from the SABnzbd clients in Sonarr and Radarr (`develop`)
  and HolyArrs (`main`). `client` names which one sends the request in this form.

Replace a file with a recorded one when a real SABnzbd is captured; keep the `exact` pointers
that still hold.

## Recorded from clients

`recorded/<client>/` holds what Sonarr, Radarr and HolyArrs actually sent to Fangst in the
end-to-end test (`tests/adapters`), one file per distinct request, with Fangst's answer, which the
client accepted: the run imported its releases. `"source": "recorded"`, `client` and
`client_version` say who sent it. These record the requests as real clients send them; the
answers are Fangst's, not SABnzbd's. `just test-adapters` then `just record-adapter-fixtures`
writes them again.

`recorded_client_requests_are_understood` in the fixture tests replays each request against the
jobs above. Answers that depend on which jobs exist (`"job_specific": true`) only have to be
understood (no 400, 403, 415 or 5xx unless the recorded answer was one); the others must match
in status and JSON shape.

## Jobs behind the fixtures

| Fangst id | Name | Category | Status | Priority | Bytes |
| --- | --- | --- | --- | --- | --- |
| 1 | `TV.Show.S04E11.720p.HDTV.x264` | tv | downloading | 0 | 1339755315 |
| 2 | `TV.Show.S04E12.720p.HDTV.x264` | tv | paused | 0 | 1339870720 |
| 3 | `Movie.2020.1080p.BluRay.x264` | movies | queued | 1 | 8589934592 |
| 4 | `TV.Show.S04E02.720p.BluRay.x264-xHD` | tv | completed | 0 | 2436906376 |
| 5 | `TV.Show.S04E03.720p.BluRay.x264-xHD` | tv | failed | 0 | 2436906376 |

Job 1 has 7600000 bytes on disk and downloads at 1327124 bytes per second with 1004 seconds left.
The download root is `/data/downloads`; job 4 finished into `/data/downloads/4-TV.Show.S04E02.720p.BluRay.x264-xHD`.
The API key is `fixture-api-key-0000`, a placeholder.
