---
fangst-adapter-sabnzbd: minor
type: feat
---

Sonarr, Radarr and other SABnzbd clients can use fangst-dl as their usenet download client after `--enable adapter-sabnzbd`, and `POST /api/v1/jobs` accepts `{ "url": "..." }` to fetch an NZB by URL.
