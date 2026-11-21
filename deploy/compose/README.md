# fangst-dl with Docker Compose

`fangst-dl.yml` runs the download daemon from `ghcr.io/fangst-media/fangst-dl`. The image is built for linux/amd64 and linux/arm64.

```sh
echo "FANGST_API_KEY=$(openssl rand -hex 24)" > .env
docker compose -f fangst-dl.yml up -d
curl http://localhost:7740/api/v1/health
```

The API listens on port 7740. The daemon does not serve the web app yet; until it does, use the API (`docs/api.md`) or the qBittorrent and SABnzbd adapters.

## Settings

| Setting | Default | What it does |
| --- | --- | --- |
| `FANGST_API_KEY` | none, required | The first user's API key, at least 16 characters. It replaces the key from the previous start. |
| `PUID`, `PGID` | `1000` | User and group the daemon runs as. Files in `/data` are handed to this user at start. `0` is refused. |
| `TZ` | `Etc/UTC` | Time zone, as a tz database name such as `Europe/Copenhagen`. |
| `command` | none | Extra `fangst-dl serve` flags, added after the container defaults. A secret named by a `--*-env` flag goes in `environment`. |

Running with `user: "1000:1000"` instead of `PUID` and `PGID` also works; the container then skips the user switch and the folders must already belong to that user.

## Volumes and ports

| Path or port | What it holds |
| --- | --- |
| `/data` | Database, master key and torrent state. Back this up. |
| `/downloads` | Downloaded files. The host folder must be writable by `PUID`; the container does not change its owner. |
| `7740/tcp` | API, including `/api/v1/health` without a key. |
| `6881/tcp`, `6881/udp` | Torrent peers and the DHT. Forward both on the router; UPnP does not reach the host from a bridge network. |

The daemon reports file paths as `/data/downloads/...`, because it keeps downloads under its data folder until the config file can set another folder (#31); in the container that path is a link to `/downloads`. A media manager in another container needs the same host folder mounted at `/data/downloads`, or a remote path mapping from `/data/downloads` to its own path.

## Usenet

```yaml
    environment:
      NEWS_PASSWORD: ${NEWS_PASSWORD}
    command: --host news.example.com --user me --password-env NEWS_PASSWORD --connections 20
```

## Building the image yourself

From the repo root:

```sh
docker build -f deploy/docker/Dockerfile -t fangst-dl .
```
