# Adapter end-to-end test

Real Sonarr, Radarr and HolyArrs add Fangst as a SABnzbd and as a qBittorrent download client,
run their own connection tests, grab a release and import it. Everything runs in Docker on one
network; nothing is published to the host.

```sh
just test-adapters            # all apps, both adapters
just test-adapters sabnzbd    # Sonarr and Radarr over the SABnzbd adapter
just test-adapters qbittorrent
just test-adapters holyarrs
just test-adapters all --keep # leave the stack running to look around; `docker compose -f tests/adapters/compose.yml down -v` ends it
```

The first run builds `fangst-dl` and the mocks in Docker, which takes several minutes; later runs
reuse Docker's build cache. A run after that takes about two minutes. It needs internet for the images and for Sonarr's and Radarr's metadata lookups.
CI (`.github/workflows/adapters.yml`) runs `all` on pull requests that touch the adapters, the
download API, `fangst-dl` or this folder, and every night.

## Parts

| Service | Image | What it does |
| --- | --- | --- |
| `fangst` | built from this repo (`Dockerfile`, target `fangst`) | `fangst-dl serve` with both adapters, the mock usenet server as its only server, DHT and local discovery off |
| `mocks` | built from this repo (target `mocks`, `tools/e2e-mocks`) | Mock NNTP server holding every usenet release, an HTTP tracker, and a seeder for every torrent release. Writes the NZB and `.torrent` files for the indexer |
| `stub` | `oven/bun:1.4.5-slim` | `stub/main.ts`: a Newznab and Torznab indexer (`indexer:9117`), a TMDB stand-in for HolyArrs (`tmdb:9118`), and one recording proxy per client in front of Fangst (`recorder:8081` Sonarr, `8082` Radarr, `8083` HolyArrs) |
| `sonarr` | `lscr.io/linuxserver/sonarr:4.0.20.3014-ls6` | API key set through `SONARR__AUTH__APIKEY` |
| `radarr` | `lscr.io/linuxserver/radarr:7.0.3.10918-ls328` | API key set through `RADARR__AUTH__APIKEY` |
| `holyarrs` | `holyarrs/holyarrs:1.2.0` | Looks titles up on the TMDB stand-in (`HOLYARRS_TMDB_URL`) |
| `driver` | `oven/bun:1.4.5-slim` | `driver/main.ts`: configures each app through its API and checks the results |
| `prepare` | built runtime image | Creates `/data` with owner 1000:1000 and exits |

Downloads (`/data/fangst/downloads`) and libraries (`/data/tv`, `/data/movies`,
`/data/holyarrs/...`) share one volume mounted at `/data` everywhere, so paths match without
remote path mappings and imports are moves or hard links.

## What the driver checks

For each app: the download client test passes for every adapter in the suite (Sonarr and Radarr's
`POST /api/v3/downloadclient/test`, HolyArrs' `POST /api/v1/downloaders/{id}/test`), the indexer
test passes, then a search grabs the app's release for each protocol and the episode or movie ends
up imported (`hasFile`, or slot state `in_library` in HolyArrs). The release must have reached
Fangst over the adapter for its protocol, read from the recording proxy's log, and Sonarr and
Radarr must name the matching download client in their grab history.

| App | Usenet release | Torrent release |
| --- | --- | --- |
| Sonarr | Breaking Bad S01E01 | Breaking Bad S01E02 |
| Radarr | The Matrix (1999) | Night of the Living Dead (1968) |
| HolyArrs | The Matrix (1999) | Breaking Bad S01E03 |

Every release is the same 11-minute, 1920x1080, black and silent video (`media/sample.mkv`,
117 KB). The clients check for samples by running time (Sonarr and Radarr want at least 10
minutes for an hour-long episode or a movie), refuse files without an audio track, and HolyArrs
compares the resolution with the release name. The indexer reports realistic sizes
(`reportedBytes` in `releases.json`), since every client refuses a 1080p release of a few hundred
kilobytes; HolyArrs is told not to compare the claimed and the downloaded size
(`refuse_tiny_releases: false`). The video was made with:

```sh
docker run --rm -v "$PWD/media:/out" linuxserver/ffmpeg:9.0.2-cli-ls90 \
  -f lavfi -i color=c=black:s=1920x1080:r=1:d=660 -f lavfi -i anullsrc=r=8000:cl=mono -t 660 \
  -c:v libx264 -preset veryslow -crf 51 -g 660 -pix_fmt yuv420p \
  -c:a libopus -b:a 6k -frame_duration 120 -dtx 1 \
  -map_metadata -1 -fflags +bitexact -flags:v +bitexact -flags:a +bitexact /out/sample.mkv
```

## Recorded fixtures

Every request a client sends to Fangst, and Fangst's answer, goes to `.records/<client>.jsonl`.
`just record-adapter-fixtures` turns the last run's log into
`fixtures/adapters/<adapter>/recorded/<client>/`, one file per distinct request, with credentials
replaced by the fixture placeholders. The adapter fixture tests replay them
(`recorded_client_requests_are_understood`). `.records/compose.log` holds every container's log.

## Credentials

All keys and passwords in `compose.yml` and the driver are throwaway values for this network.
