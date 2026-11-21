# Fangst

**Self-hosted media tools that bring in the catch and leave your files alone.**

Fangst is Danish for "the catch". It's a self-hosted media platform written in Rust, built in two parts:

- **A download daemon** (`fangst-dl`) for usenet and torrents in one process. It speaks the qBittorrent and SABnzbd APIs, so Sonarr, Radarr and similar apps can use it without changes, replacing two separate download clients.
- **A media manager** for movies and TV on top of it, with a headless API that any frontend can use.

> Fangst is in early development and not ready for general use.

## Your files stay yours

- **Original filenames are kept.** Imports use hardlinks where possible, so seeding keeps working. Renaming is optional.
- **Your folder structure stays.** Adopting an existing library moves and renames nothing.
- **Metadata travels with the files.** Kodi/Jellyfin NFO and Fangst JSON sidecars sit next to your media, so a lost database or a switch to another app costs you nothing.

## Status

| Part | State |
|---|---|
| Usenet downloads (NNTP, yEnc, NZB, resume after restart, job timeline) | working |
| Native HTTP API with OpenAPI, live event stream, web queue page | working |
| Torrents (magnet links and `.torrent` files, on by default) | working |
| PAR2 repair, qBittorrent and SABnzbd compatibility | working |
| Unpacking | planned |
| Media manager | planned |

The plan lives in the [issues](https://github.com/Fangst-Media/fangst/issues) and milestones.

## Development

You need Rust (the pinned version installs itself through `rust-toolchain.toml`), Bun, `just`, `cargo-nextest`, `cargo-deny` and `changesetter`.

```bash
just setup        # install JS dependencies and fetch crates
just check-full   # the gate: lint, tests, layer and licence checks, generated files
just web-dev      # web app with hot refresh

# the download daemon, API on 127.0.0.1:7740; settings in data/fangst.toml (see TUNING.md)
FANGST_API_KEY=<16 or more characters> cargo run -p fangst-dl -- serve --data-dir data --host news.example.com
```

[AGENTS.md](AGENTS.md) has every command, the architecture rules and the code style.

## Contributing

Contributions are welcome, written by hand or with a coding agent. Start at [AGENTS.md](AGENTS.md) and the [Contributing Guide](CONTRIBUTING.md). By opening a pull request you agree to the [contributor licence agreement](CLA.md).

## Licence

The application is under the [GNU Affero General Public License v3.0 or later](LICENSE). Low-level reusable crates (NNTP, yEnc, NZB, release-name parsing, provider traits) are under MIT OR Apache-2.0 so other projects can use them. The name and logo are not covered by the code licences; see [TRADEMARKS.md](TRADEMARKS.md).

## Alternatives

- **[HolyArrs](https://github.com/HolyArrs/HolyArrs)**
- **[MediaManager](https://github.com/maxdorninger/MediaManager/)**
- **[Cinephage](https://github.com/MoldyTaint/Cinephage)**
- **[Scryer](https://github.com/scryer-media/scryer)**

## Acknowledgments

- [Sonarr](https://sonarr.tv) and [Radarr](https://radarr.video), the originals
- [Prowlarr](https://prowlarr.com) for indexer management
- [The Movie Database (TMDB)](https://www.themoviedb.org/) for movie and TV metadata. This product uses the TMDB API but is not endorsed or certified by TMDB.
- [Dictionarry](https://dictionarry.dev/) for quality scoring and release group tier data
- The whole \*arr community

<p align="center">
  <sub>Built with ☕ in Copenhagen</sub>
</p>
