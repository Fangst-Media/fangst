# 0007. Seeding is a flag on completed torrent jobs, with limits per category

Status: accepted
Date: 2026-11-21
Issue or PR: #25

## Context
Until #25 a finished torrent left the engine and stopped uploading at once. Torrent users expect a
client to keep seeding up to a ratio or a time, and private trackers require it. A seeding job is
done for everything downstream (import, notifications, the files are final), yet it still holds a
torrent in the engine and changes counts. The job model (0001) has five statuses; the question was
whether seeding is a sixth.

## Decision
- **Status stays `completed`, with a `seeding` flag.** `JobDto` gains `seeding`, `uploaded_bytes`,
  `ratio` and `seeding_secs`; `GET /api/v1/jobs/{id}/torrent` adds the limits that apply. A client
  that only knows the five statuses sees a finished job, which is what it is: the files are in
  place and can be imported while seeding goes on.
- **Limits:** ratio (uploaded bytes over the torrent's size, downloading uploads included) and seed
  time (counted from when the download finished, only while the daemon runs), whichever comes
  first. Defaults live in `[seeding]` of `defaults.toml`: ratio 1.0, 7 days. A category can set
  either limit in `categories` (`PUT /api/v1/categories/{name}/seeding`); `null` uses the default,
  a negative value turns that limit off. A seeding job reads its limits again each time it writes
  its counts, so a changed category applies without a restart.
- **Stopping** removes the torrent from the engine and keeps the files. Timeline entries of kind
  `seeding` say when it starts (with the limits), when a restart picks it back up (with the time
  already counted) and why it stops.
- **Persistence:** `torrent_jobs` holds `uploaded_bytes`, `seeding_secs` and `seeding`, written
  every `progress.persist_interval_secs` and when seeding stops. On start the daemon puts every
  job with `seeding = 1` back in the engine after a hash check; files that no longer pass end
  seeding with that reason.
- **Slots:** seeding jobs don't count against `torrent.max_active` (0005); the slot is freed when
  the download finishes.
- **qBittorrent adapter:** a seeding job is `uploading` or `stalledUP` with its real limits; a job
  done seeding is `stoppedUP` and reports the limit it reached, so Sonarr and Radarr's "remove
  completed" removes it once seeding ends and not before.

## Alternatives
- **A `seeding` status.** Clearer in a status list, but every client, adapter and the
  notification path treat "completed" as the end of a job, and SQLite needs a table rebuild to
  widen the `jobs.status` check. A torrent that seeds would look unfinished to anything that
  waits for `completed`.
- **Per-job limits from the add request.** Sonarr sends `ratioLimit` and `seedingTimeLimit` per
  torrent from its indexer settings; Fangst ignores them for now. A per-job override would sit
  beside the category one.
- **Counting seeds against the download cap.** Five long seeds would block every new download.

## Consequences
- Uploads of a download interrupted by a restart are not counted; only a run that reaches seeding
  writes them.
- A disk that is not mounted when the daemon starts ends seeding for its jobs for good, with the
  hash-check reason on the timeline.
- No way yet to stop seeding by hand short of deleting the job, or to start it again once stopped.
