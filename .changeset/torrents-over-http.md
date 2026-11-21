---
fangst-dl: minor
type: feat
---

`fangst-dl serve` downloads torrents: add a `.torrent` file, a magnet link or a URL to one through `POST /api/v1/jobs` or the web queue. The engine is on by default (`--no-torrent` turns it off) and runs at most 5 torrents at once (`--max-active-torrents`).
