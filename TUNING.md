# Tuning

Every value Fangst uses that you might want to change: connection counts, timeouts, retry backoff, seeding limits and the like. The built-in defaults live in `crates/fangst-core/defaults.toml`; your own config file overrides any of them.

Each entry says what the value does, its default, its allowed range, and when you'd change it. Entries are added together with the code that reads them.

## `[notify]`

### `timeout_secs`

How long one notifier (webhook, ntfy, Discord) may take to deliver one event before it counts as failed. A failed notification is written to the job's timeline as a `notify` entry; the job itself is never affected.

- Default: `10`
- Range: 1 or more
- Change it when a self-hosted notification service is slow to answer and you see timeouts on job timelines.

## `[disk]`

### `min_free_bytes`

When free space on the disk holding the downloads folder drops below this many bytes, the enabled notifiers get one disk-full notification. The next one goes out only after free space has gone back above the value and dropped below it again.

- Default: `5368709120` (5 GiB)
- Range: 0 (never) or more
- Raise it when your downloads are large, so the warning arrives while there is still room to finish the current job.

### `check_interval_secs`

How often free space in the downloads folder is checked. Only runs while at least one notifier is enabled.

- Default: `60`
- Range: 1 or more
- Lower it when the disk fills quickly, for example with fast connections and small disks.

## `[progress]`

### `publish_interval_ms`

How often a running job's downloaded bytes, rate and time left go out on the event stream (`/api/v1/events`) and are measured for the API. Each running job sends at most one progress message per interval.

- Default: `1000`
- Range: 1 or more
- Raise it when many clients watch the event stream over a slow link.

### `rate_window_secs`

The download rate is the bytes downloaded over this many recent seconds, divided by the time they took. A longer window gives a steadier rate and time left; a shorter one follows changes faster.

- Default: `5`
- Range: 1 or more
- Raise it when your connection's speed jumps around and the time left jumps with it.

### `persist_interval_secs`

How often a running job's downloaded bytes are written to the database, so the queue shows a sensible number after a restart. Bytes are also written whenever a job stops, pauses or finishes.

- Default: `10`
- Range: 1 or more
- Lower it if a crash should lose less of the shown progress; the downloaded data itself is never lost by this.
