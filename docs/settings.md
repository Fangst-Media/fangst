# Jaeger Settings Guide

Jaeger uses a layered configuration system that supports both file-based and environment variable configuration.

## Configuration Priority

Settings are loaded in this order (later overrides earlier):

1. **Defaults** - Sensible fallback values
2. **YAML File** (`./data/settings.yaml`) - Primary configuration
3. **Environment Variables** - Override YAML settings and lock them in the UI

## Settings File Location

The settings file is located at `./data/settings.yaml` relative to the API package working directory.

On first run, Jaeger will:
1. Create the `data/` directory if it doesn't exist
2. Generate an API key and save it to `settings.yaml`

## Environment Variables

Environment variables take precedence over the YAML file and **lock** the corresponding settings in the UI (shown with an "ENV" badge).

| Environment Variable | Settings Path | Type | Default |
|---------------------|---------------|------|---------|
| `PORT` | `server.port` | number | 5001 |
| `CORS_ORIGIN` | `server.corsOrigin` | string | http://localhost:5173 |
| `DATABASE_URL` | `database.url` | string | ./data/jaeger.db |
| `JAEGER_API_KEY` | `security.apiKey` | string | (auto-generated) |
| `SESSION_EXPIRY_DAYS` | `security.sessionExpiryDays` | number | 30 |
| `JELLYFIN_URL` | `auth.jellyfin.url` | string | http://localhost:8096 |
| `SYNC_ADMIN_FROM_PROVIDER` | `auth.jellyfin.syncAdminFromProvider` | boolean | true |
| `MOCK_AUTH` | `auth.mock.enabled` | boolean | false |
| `MEDIA_PATH` | `media.path` | string | /media |
| `MOCK_FILESYSTEM` | `media.mockFilesystem` | boolean | false |
| `METADATA_PROVIDERS` | `metadata.providers` | string (comma-separated) | relay,tmdb,tvdb |
| `METADATA_RELAY_URL` | `metadata.relay.url` | string | https://metadata-relay.dorninger.co |
| `TMDB_API_KEY` | `metadata.tmdb.apiKey` | string | (empty) |
| `TVDB_API_KEY` | `metadata.tvdb.apiKey` | string | (empty) |

## API Key

The API key is used for external integrations and programmatic access to the Jaeger API.

**Automatic Generation**: If no API key is provided via environment variable, Jaeger generates one automatically on first startup and saves it to `settings.yaml`.

**Regeneration**: Admins can regenerate the API key from the Settings page (unless locked by ENV).

**Security**: The API key is masked in the UI. Use the "Show" button to reveal it, and "Copy" to copy to clipboard.

## Example Configuration

### Minimal YAML (settings.yaml)

```yaml
# Most settings use sensible defaults
# Only the API key is required (auto-generated if missing)
security:
  apiKey: "your-generated-api-key"
```

### Full YAML Configuration

```yaml
server:
  port: 5001
  corsOrigin: "http://localhost:5173"

database:
  url: "./data/jaeger.db"

security:
  apiKey: "your-api-key-here"
  sessionExpiryDays: 30

auth:
  provider: jellyfin
  jellyfin:
    url: "http://jellyfin.local:8096"
    syncAdminFromProvider: true
  mock:
    enabled: false

media:
  path: "/mnt/media"
  mockFilesystem: false

metadata:
  providers:
    - relay
    - tmdb
    - tvdb
  relay:
    url: "https://metadata-relay.dorninger.co"
  tmdb:
    apiKey: "your-tmdb-key"
  tvdb:
    apiKey: "your-tvdb-key"

ui:
  theme: dark
  language: en
  defaultLibraryView: grid
```

### Docker Compose with Environment Variables

```yaml
services:
  jaeger:
    image: jaeger:latest
    ports:
      - "5001:5001"
    environment:
      - MEDIA_PATH=/media
      - JELLYFIN_URL=http://jellyfin:8096
      - TMDB_API_KEY=your-tmdb-key
    volumes:
      - ./data:/app/data
      - /path/to/media:/media:ro
```

## Settings Categories

### Server
- **port**: API server port
- **corsOrigin**: Allowed CORS origin for the web frontend

### Security
- **apiKey**: API authentication key for external access
- **sessionExpiryDays**: How long user sessions remain valid (1-365 days)

### Authentication
- **provider**: Authentication provider (`jellyfin` or `mock`)
- **jellyfin.url**: Jellyfin server URL
- **jellyfin.syncAdminFromProvider**: Sync admin status from Jellyfin
- **mock.enabled**: Enable mock authentication (for development)

### Media
- **path**: Root directory for media files
- **mockFilesystem**: Use mock filesystem (for development/testing)

### Metadata
- **providers**: Array of metadata providers in priority order
- **relay.url**: Metadata relay service URL
- **tmdb.apiKey**: TMDB API key (get one at https://www.themoviedb.org/settings/api)
- **tvdb.apiKey**: TVDB API key

### UI
- **theme**: Color theme (`dark`, `light`, or `system`)
- **language**: Interface language (`en`, `da`, `de`)
- **defaultLibraryView**: Default library view mode (`grid`, `list`, `table`)

## Locked Settings

When a setting is provided via environment variable:
- It overrides any value in the YAML file
- It appears with an "ENV" badge in the Settings UI
- It cannot be modified through the UI
- The YAML file value is preserved but ignored

This allows deployment flexibility: use ENV for production secrets and deployment-specific settings, while keeping the YAML file for user-configurable options.
