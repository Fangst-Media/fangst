# 0005. The torrent engine runs in fangst-dl by default, with a cap on active torrents

Status: accepted
Date: 2026-11-21
Issue or PR: #97

## Context
#24 added the torrent engine (`fangst-torrent`) and torrent jobs in the queue, but the daemon
neither started the engine nor accepted torrents over HTTP. The queue's worker also only ran when a
usenet server was configured, so a torrent-only setup would never have started a job.

## Decision
- **On by default.** `fangst-dl serve` starts one engine for all torrent jobs, with its state
  (known DHT nodes) in `<data-dir>/torrent`. `--no-torrent` leaves it off; torrent jobs can still be
  added and wait in the queue. Flags cover the listen port (`--torrent-port`, default from
  `torrent.listen_port`), an announce port for forwarded ports, `--no-dht`,
  `--no-local-discovery`, `--no-upnp`, and download and upload limits in bytes per second.
- **The worker always runs.** Each protocol lane idles on its own when its engine or server is
  missing, so torrents run without a usenet server and the other way round.
- **A cap on active torrents.** At most `torrent.max_active` torrent jobs (default 5,
  `--max-active-torrents`) download at once; the rest stay queued in priority order. A magnet
  link waiting for its metadata holds a slot, since the engine already works on it.
- **Input.** `POST /api/v1/jobs` takes a `.torrent` body (`application/x-bittorrent`), a magnet
  link as `{"magnet": "..."}` or `?magnet=`, and `{"url": "..."}` where the URL may answer with a
  `.torrent` (recognised by its bencoded first byte) or redirect to a magnet link, as torrent
  indexers do. A redirect to `magnet:` is the only non-http(s) redirect the fetcher accepts, and
  it is not followed: the link is added as a magnet job. A torrent's metadata is checked where it
  enters, file names included (#24).

## Alternatives
- **Off by default.** Safer for usenet-only users, but the daemon is meant to replace both kinds of
  client, and a fresh install that silently ignores torrents is a worse surprise.
- **No cap, as before.** Every torrent splits the same bandwidth and disk, and dozens of dead
  magnet links would each keep peer connections open.
- **Counting seeding torrents against the cap.** Finished torrents leave the engine today, so there
  is nothing to count yet; revisit with seeding.

## Consequences
- **SSRF through trackers.** Adding a torrent or magnet link makes the daemon announce to every
  tracker it names, over HTTP, HTTPS or UDP, and connect to the peers trackers hand back. A
  tracker URL can point at any address the daemon can reach, including the LAN, loopback and
  cloud metadata endpoints, and the announce is a GET with query parameters the torrent chooses
  in part. Unlike the NZB fetch (0004), nothing restricts the scheme or the destination, and peers
  can be any address. The response is not shown to the caller. Only API key holders and signed-in
  users can add torrents, and they already control the daemon; deployments that hand keys to
  people they don't trust need egress filtering, for example a firewall that only lets the daemon
  reach the internet.
- DHT, local discovery and `UPnP` reach the network on their own as soon as the daemon starts.
  Tests and the browser smoke test turn them off (or the engine) so nothing leaves the machine.
- The default port 6881 must be free; a second daemon on the same host needs `--torrent-port`.
