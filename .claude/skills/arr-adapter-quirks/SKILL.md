---
name: arr-adapter-quirks
description: What Sonarr, Radarr, HolyArrs, Seerr and similar clients actually send to qBittorrent, SABnzbd and Sonarr/Radarr APIs, where it differs from the upstream docs. Read before changing any fangst-adapter-* crate.
---

# Adapter quirks

Clients depend on the behaviour of the real qBittorrent, SABnzbd, Sonarr and Radarr, not on their documentation. Every difference found goes here, with the client, the request, what the client expects and the recorded fixture that proves it.

Format for each entry:

```markdown
## Client: short description

**Request:** method, path and the parameters that matter.
**Expects:** the response shape or behaviour the client relies on.
**Fixture:** `fixtures/adapters/<adapter>/<file>`.
**Handled in:** `crates/fangst-adapter-<name>/src/<file>.rs`.
```

No entries yet.
