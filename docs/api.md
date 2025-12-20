# Jaeger API Documentation

Base URL: `http://localhost:5001`

## Authentication

There are two ways to authenticate:

### 1. Session-based (for web UI)
Login via `/api/auth/login` to obtain a session cookie.

### 2. API Key (for programmatic access)
Use the Jaeger API key for scripts, integrations, and testing. The API key grants admin access.

**Header method (recommended):**
```bash
curl -H "X-Api-Key: your-api-key" http://localhost:5001/api/media
```

**Query parameter method:**
```bash
curl "http://localhost:5001/api/media?apiKey=your-api-key"
```

Get your API key from Settings > Security in the web UI, or check `data/settings.yaml`.

## Endpoints

### Health Check

#### GET /health

Check API health status.

**Response:**
```json
{
  "status": "ok"
}
```

---

### Authentication

#### POST /api/auth/login

Authenticate with a media server.

**Request:**
```json
{
  "provider": "jellyfin",
  "username": "user",
  "password": "pass"
}
```

**Response:**
```json
{
  "user": {
    "id": "uuid",
    "name": "username",
    "role": "admin"
  }
}
```

#### GET /api/auth/me

Get current authenticated user.

**Response:**
```json
{
  "user": {
    "id": "uuid",
    "name": "username",
    "role": "admin"
  }
}
```

#### POST /api/auth/logout

End current session.

**Response:**
```json
{
  "success": true
}
```

---

### Media

#### GET /api/media

Get all media items from the database.

**Response:**
```json
{
  "results": [
    {
      "id": "uuid",
      "path": "/media/Movie.2024.1080p.mkv",
      "filename": "Movie.2024.1080p.mkv",
      "parsed": {
        "title": "Movie",
        "year": 2024,
        "resolution": "1080p"
      },
      "metadata": {
        "id": "tmdb:12345",
        "title": "Movie Title",
        "year": 2024,
        "overview": "...",
        "posterPath": "/path.jpg",
        "rating": 8.5,
        "provider": "tmdb"
      }
    }
  ]
}
```

#### POST /api/media/scan

Trigger a media library scan. **Requires admin role.**

**Response:**
```json
{
  "mockFilesystem": false,
  "providers": ["relay", "tmdb", "tvdb"],
  "scannedPath": "/media",
  "sync": {
    "added": 5,
    "updated": 2,
    "removed": 1
  },
  "results": [...]
}
```

---

### Settings

#### GET /api/settings

Get all settings with lock status. **Requires admin role.**

Sensitive values (API keys) are masked in the response.

**Response:**
```json
{
  "settings": {
    "server": {
      "port": 5001,
      "corsOrigin": "http://localhost:5173"
    },
    "database": {
      "url": "./data/jaeger.db"
    },
    "security": {
      "apiKey": "abcd********wxyz",
      "sessionExpiryDays": 30
    },
    "auth": {
      "provider": "jellyfin",
      "jellyfin": {
        "url": "http://localhost:8096",
        "syncAdminFromProvider": true
      },
      "mock": {
        "enabled": false
      }
    },
    "media": {
      "path": "/media",
      "mockFilesystem": false
    },
    "metadata": {
      "providers": ["relay", "tmdb", "tvdb"],
      "relay": {
        "url": "https://metadata-relay.dorninger.co"
      },
      "tmdb": {
        "apiKey": "********"
      },
      "tvdb": {
        "apiKey": ""
      }
    },
    "ui": {
      "theme": "system",
      "language": "en",
      "defaultLibraryView": "grid"
    }
  },
  "locked": {
    "media": {
      "path": true
    },
    "metadata": {
      "tmdb.apiKey": true
    }
  }
}
```

#### GET /api/settings/ui

Get UI settings only. **No authentication required.**

**Response:**
```json
{
  "ui": {
    "theme": "system",
    "language": "en",
    "defaultLibraryView": "grid"
  }
}
```

#### PATCH /api/settings

Update settings. Locked settings are ignored. **Requires admin role.**

**Request:**
```json
{
  "ui": {
    "theme": "dark"
  },
  "security": {
    "sessionExpiryDays": 60
  }
}
```

**Response:**
```json
{
  "success": true,
  "settings": {...},
  "locked": {...}
}
```

#### GET /api/settings/api-key

Get the full (unmasked) API key. **Requires admin role.**

**Response:**
```json
{
  "apiKey": "full-api-key-here",
  "locked": false
}
```

#### POST /api/settings/api-key/regenerate

Generate a new API key. Fails if locked by environment variable. **Requires admin role.**

**Response:**
```json
{
  "success": true,
  "apiKey": "new-api-key-here"
}
```

---

## Error Responses

All endpoints may return error responses:

```json
{
  "error": "Error message here"
}
```

Common HTTP status codes:
- `400` - Bad request (invalid input)
- `401` - Unauthorized (not logged in)
- `403` - Forbidden (insufficient permissions)
- `500` - Internal server error

---

### Downloaders

#### GET /api/downloaders

List all configured download clients. **Requires admin role.**

**Response:**
```json
{
  "downloaders": [
    {
      "id": "uuid",
      "name": "Main SABnzbd",
      "type": "sabnzbd",
      "url": "http://localhost:8080",
      "apiKey": "********",
      "enabled": true,
      "categories": {
        "movies": "movies",
        "tv": "tv"
      }
    }
  ]
}
```

#### POST /api/downloaders

Add a new download client. **Requires admin role.**

**Request:**
```json
{
  "name": "Main SABnzbd",
  "type": "sabnzbd",
  "url": "http://localhost:8080",
  "apiKey": "your-api-key",
  "enabled": true,
  "categories": {
    "movies": "movies",
    "tv": "tv"
  }
}
```

**Response:**
```json
{
  "success": true,
  "downloader": {
    "id": "generated-uuid",
    "name": "Main SABnzbd",
    "type": "sabnzbd",
    "url": "http://localhost:8080",
    "apiKey": "********",
    "enabled": true,
    "categories": {
      "movies": "movies",
      "tv": "tv"
    }
  }
}
```

#### GET /api/downloaders/:id

Get a specific downloader by ID. **Requires admin role.**

**Response:**
```json
{
  "downloader": {
    "id": "uuid",
    "name": "Main SABnzbd",
    "type": "sabnzbd",
    "url": "http://localhost:8080",
    "apiKey": "********",
    "enabled": true
  }
}
```

#### PATCH /api/downloaders/:id

Update a downloader. **Requires admin role.**

**Request:**
```json
{
  "name": "Updated Name",
  "enabled": false
}
```

**Response:**
```json
{
  "success": true,
  "downloader": {...}
}
```

#### DELETE /api/downloaders/:id

Delete a downloader. **Requires admin role.**

**Response:**
```json
{
  "success": true
}
```

#### POST /api/downloaders/:id/test

Test connection to a saved downloader. **Requires admin role.**

**Response:**
```json
{
  "success": true,
  "version": "4.2.1"
}
```

Or on failure:
```json
{
  "success": false,
  "error": "Connection refused"
}
```

#### POST /api/downloaders/test

Test connection with provided credentials (for add/edit form). **Requires admin role.**

**Request:**
```json
{
  "type": "sabnzbd",
  "url": "http://localhost:8080",
  "apiKey": "your-api-key"
}
```

**Response:**
```json
{
  "success": true,
  "version": "4.2.1"
}
```

---

## Role-Based Access

- **Public**: Health check, UI settings
- **Authenticated**: Media browsing, requests
- **Admin**: Dashboard, library scanning, settings management, downloader configuration
