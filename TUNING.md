# Tuning

Every value Fangst uses that you might want to change: connection counts, timeouts, retry backoff, seeding limits and the like. The built-in defaults live in `crates/fangst-core/defaults.toml`; your own config file overrides any of them.

Each entry says what the value does, its default, its allowed range, and when you'd change it. Entries are added together with the code that reads them.

## Where values come from

Four layers, each overriding the one before, key by key:

1. The built-in defaults (`crates/fangst-core/defaults.toml`).
2. `<data-dir>/fangst.toml`. The daemon writes a short one on first start, and the web UI rewrites it when settings change, so comments in it are not kept. Put any value from this page under its own section, for example `[torrent]` and `max_active = 3`.
3. Environment variables named `FANGST_<SECTION>__<KEY>`, with two underscores between section and key: `FANGST_TORRENT__MAX_ACTIVE=3`. Deeper settings take more pairs, as in `FANGST_NOTIFIERS__NTFY__TOPIC=fangst`. A value that reads as `true`, `false`, a whole number or a TOML array (`["a", "b"]`) takes that type; anything else is text. Quote text that looks like a number: `FANGST_NOTIFIERS__NTFY__TOPIC='"123"'`.
4. `fangst-dl serve` flags such as `--torrent-port`.

A value set by layer 3 or 4 shows as locked in the web UI and can't be changed there. Usenet passwords and notifier tokens never go into `fangst.toml`: the web UI stores them encrypted in `downloads.db`, or a setting names an environment variable that holds one (`password_env`, `token_env`, `webhook_url_env`).

## `[notify]`

### `timeout_secs`

How long one notifier (webhook, ntfy, Discord) may take to deliver one event before it counts as failed. A failed notification is written to the job's timeline as a `notify` entry; the job itself is never affected.

- Default: `10`
- Range: 1 or more
- Change it when a self-hosted notification service is slow to answer and you see timeouts on job timelines.

### `ntfy_server`

The ntfy server `notify-ntfy` publishes to when its settings name none.

- Default: `"https://ntfy.sh"`
- Range: an `http` or `https` URL
- Change it when you run your own ntfy server and want every install to use it without setting `notifiers.ntfy.server`.

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

## `[torrent]`

### `max_active`

How many torrent jobs download at the same time. Further torrent jobs stay queued and start, highest priority first, when a running one finishes, fails, is paused or is deleted. A magnet link counts as running while the daemon asks peers for its file list, so a few links nobody seeds can hold every slot until you pause or delete them. Usenet jobs are not counted; they run one at a time in their own lane.

- Default: `5`
- Range: 1 or more
- Raise it when you add many torrents with few seeders each, so slow swarms don't keep the rest waiting. Lower it on a slow line, where every extra torrent splits the same bandwidth further.

### `listen_port`

The TCP port other peers connect to. `fangst-dl serve --torrent-port` overrides it; `0` picks a free port at every start, which works for downloading but leaves peers behind a firewall unable to reach you.

- Default: `6881`
- Range: 0 to 65535
- Change it when the port is taken by another program, or to match a port forwarded on your router.

### `enabled`

Whether the torrent engine runs. Off, torrent jobs can still be added and wait in the queue. `--no-torrent` turns it off.

- Default: `true`
- Range: `true` or `false`
- Turn it off on a usenet-only setup, so the daemon opens no peer port.

### `announce_port`

The port told to trackers and the DHT, when peers reach this machine on another port than `listen_port`, such as one forwarded by a VPN. `0` announces `listen_port`.

- Default: `0`
- Range: 0 to 65535
- Set it to the forwarded port when a VPN or router maps a different outside port to `listen_port`.

### `dht`, `local_discovery`, `upnp`

Ways to find peers and open the port: the DHT (the tracker-less peer network), multicast on the local network, and asking the router to forward `listen_port`. Each reaches the network on its own as soon as the daemon starts.

- Default: `true` for each
- Range: `true` or `false`
- Turn them off on private trackers that forbid the DHT, or when nothing should leave the machine unasked.

### `download_limit`, `upload_limit`

Bytes per second across all torrents. `0` means no limit.

- Default: `0`
- Range: 0 to 4294967295
- Set an upload limit when seeding fills your line's upstream and slows everything else down.

## `[extract]`

### `delete_archives`

Whether the `postproc-extract` post-processor deletes a job's archive files (every volume of every RAR and 7z archive) once all of them extracted. When any archive fails, nothing is deleted and the files extracted so far are removed again, so the job can be retried.

- Default: `true`
- Range: `true` or `false`
- Turn it off when you seed or re-share the original archives, or want to keep them until you have checked the result.

## `[safety]`

### `blocked_extensions`

File types the `postproc-safety` post-processor refuses. A job fails, naming the file, when one of these appears inside an archive or lies in the job folder itself. Extensions are written without the dot and compared ignoring case.

- Default: `["exe", "lnk", "scr", "bat", "cmd", "com", "pif", "vbs", "js", "msi", "ps1"]`
- Range: any list, including an empty one
- Add types you never expect in media downloads, such as `jar` or `hta`. Removing one lets that type through the check.

## `[usenet]`

Values a usenet server gets when its own settings leave them out, and how every server is used.

### `port`, `tls`

The port and whether to use TLS. Most providers offer TLS on 563 and plain text on 119.

- Default: `563` and `true`
- Range: 1 to 65535; `true` or `false`
- Change them only when your provider uses other ports.

### `connections`

Connections opened to one server. Your provider's plan sets the most it accepts; more than that are refused.

- Default: `8`
- Range: 1 to 200
- Raise it to what your plan allows, usually 20 to 50, for full speed on a fast line.

### `pipeline`

Requests each connection keeps in flight, so the connection never waits for the server between articles.

- Default: `8`
- Range: 1 or more
- Lower it if a provider drops connections under load.

### `connect_timeout_secs`, `command_timeout_secs`

Seconds to wait for DNS, TCP and TLS when opening a connection, and for any single reply.

- Default: `15` and `60`
- Range: 1 or more
- Raise the command timeout on slow links where large articles take long to arrive.

### `retries`

Times a segment is retried after a connection error before it counts as failed.

- Default: `3`
- Range: 0 or more
- Raise it on an unstable connection.

## `[http]`

### `listen`

The address the API and the adapters listen on. `0.0.0.0:7740` listens on every network; port `0` picks a free port. `--listen` overrides it.

- Default: `"127.0.0.1:7740"`
- Range: an address and port
- Change it to reach the daemon from other machines, such as Sonarr on another host or in another container.

### `secure_cookie`

Marks the session cookie `Secure`, so browsers only send it over HTTPS. `--secure-cookie` turns it on.

- Default: `false`
- Range: `true` or `false`
- Turn it on when the daemon is only reached through an HTTPS reverse proxy.
