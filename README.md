# Jæger

**The media hunter that respects your files.**

Jæger (Danish for "hunter") is a unified media management system that consolidates the fragmented \*arr ecosystem into a single, modern application — without forcing you to abandon your existing setup.


## Why Jæger?

The self-hosted media stack is powerful but fragmented. Running Sonarr, Radarr, Prowlarr, and Overseerr means managing 4+ separate apps, each with their own database, configuration, and quirks.

## Philosophy

### Your Files, Your Way

Jæger doesn't rename your files or force a folder structure. Quality and metadata are tracked in the database, not encoded in filenames.

- **Keep original filenames** — Seeding keeps working
- **Flexible folder structure** — We adapt to you, not the other way around
- **Optional renaming** — If you want it, it's there

### Portable Metadata

Jæger exports metadata as sidecar files alongside your media:

- **Kodi/Jellyfin NFO** — Industry standard, works everywhere
- **Jæger JSON** — Full metadata including quality, source, and release info

Lose your database? Your sidecars have you covered. Switch apps? Everything just works.

## Tech Stack

Bun, Hono, React. In the future we might add some rust services to keep things super performant while saving your RAM for something else. Maybe even an entire rust backend, but for now we keep it simple.

## Installation

> ⚠️ Jæger is in early development. Not ready for general use.

## Development

```bash
# Install dependencies
bun install

# Start development servers
bun run dev
```

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) before submitting a PR. Coding agents and people working with them start at [AGENTS.md](AGENTS.md). By opening a pull request you agree to the [contributor licence agreement](CLA.md).

## Licence

The application is under the [GNU Affero General Public License v3.0 or later](LICENSE). Low-level reusable crates (NNTP, yEnc, NZB, release-name parsing, provider traits) are under MIT OR Apache-2.0 so other projects can use them. The name and logo are not covered by the code licences; see [TRADEMARKS.md](TRADEMARKS.md).

## Alternatives

Looking for something different? Check out these projects:

- **[MediaManager](https://github.com/maxdorninger/MediaManager/)**
- **[Cinephage](https://github.com/MoldyTaint/Cinephage)**

## Acknowledgments

Jæger stands on the shoulders of giants:

- [Sonarr](https://sonarr.tv) / [Radarr](https://radarr.video) — The originals
- [Prowlarr](https://prowlarr.com) — Indexer management
- [The Movie Database (TMDB)](https://www.themoviedb.org/) — Movie and TV show metadata
- This product uses the TMDB API but is not endorsed or certified by TMDB
- [Dictionarry](https://dictionarry.dev/) — Quality scoring and release group tier data
- The entire \*arr community

---

<p align="center">
  <sub>Built with ☕ in Copenhagen</sub>
</p>
