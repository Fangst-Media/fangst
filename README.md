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

## API

Jæger is API-first. Every feature available in the UI is available via the API.

```bash
# List all media
curl http://localhost:3000/api/media

# Search
curl http://localhost:3000/api/search?q=the+bear

# Add movie
curl -X POST http://localhost:3000/api/media \
  -H "Content-Type: application/json" \
  -d '{"tmdbId": 603, "qualityProfile": "HD"}'
```

Full API documentation available at `http://localhost:3000/api/docs` when running.

### Sonarr/Radarr Compatibility

Existing tools work out of the box:

```bash
# These endpoints are compatible with LunaSea, nzb360, etc.
GET /api/v3/series
GET /api/v3/movie
GET /api/v3/queue
```

## Development

```bash
# Clone
git clone https://github.com/jaeger-media/jaeger.git
cd jaeger

# Install dependencies
pnpm install

# Start development servers
pnpm dev
```

### Project Structure

```
jaeger/
├── packages/
│   ├── api/          # Hono backend
│   ├── web/          # React SPA
│   ├── shared/       # Shared types and schemas
│   └── parser/       # Release name parser
├── docker/
└── docs/
```

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) before submitting a PR.

## Acknowledgments

Jæger stands on the shoulders of giants:

- [Sonarr](https://sonarr.tv) / [Radarr](https://radarr.video) — The originals
- [Prowlarr](https://prowlarr.com) — Indexer management
- The entire \*arr community

---

<p align="center">
  <sub>Built with ☕ in Copenhagen</sub>
</p>
