---
fangst-downloads: minor
type: feat
---

The queue and job page show a progress bar with downloaded bytes, speed and time left for every running download, updated about once a second over the event stream. The API's jobs gain downloaded_bytes, rate_bytes_per_sec and eta_seconds, and the SABnzbd adapter now reports real progress, speed and time left to Sonarr, Radarr and HolyArrs.
