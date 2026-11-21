---
fangst-dl: minor
type: feat
---

`fangst-dl serve` reads its settings from `<data-dir>/fangst.toml`, written on first start, with `FANGST_<SECTION>__<KEY>` environment variables and the existing flags on top. Several usenet servers (with priority and backfill), categories with their own folders, notifiers, torrent switches and enabled providers live there, and `GET`/`PUT /api/v1/settings` changes them while the daemon runs. Passwords and tokens are stored encrypted, never in the file.
