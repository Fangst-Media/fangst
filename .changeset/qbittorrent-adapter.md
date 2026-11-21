---
fangst-adapter-qbittorrent: minor
type: feat
---

Sonarr, Radarr and other qBittorrent clients can use fangst-dl as their torrent download client after `--enable adapter-qbittorrent`; sign in with your Fangst username and password, or any username with an API key as the password. The native API gains categories (`/api/v1/categories`, `PUT /api/v1/jobs/{id}/category`) and torrent details (`GET /api/v1/jobs/{id}/torrent`).
