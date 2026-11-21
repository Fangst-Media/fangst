# 0007. One config file layered over the defaults, secrets outside it

Status: accepted
Date: 2026-11-21
Issue or PR: #31

## Context
Everything `fangst-dl serve` knew came from flags: one usenet server, the notifiers, the torrent
switches and the enabled providers. The web UI needs to change servers, categories and notifiers
while the daemon runs (#30), a first start needs no setup at all (#31), and containers need to
configure everything from environment variables. Usenet passwords and notifier tokens must not end
up in a file people paste into bug reports.

## Decision
- **One file, `<data-dir>/fangst.toml`.** It holds the settings sections (`folders`, `servers`,
  `categories`, `notifiers`, `providers`, `setup`) and overrides of any tunable from
  `defaults.toml` under its own section. The first start writes a short one. Unknown sections
  and keys stop the daemon with their name, so a typo is never silently ignored.
- **Four layers, merged key by key** as TOML tables: built-in `defaults.toml`, the file,
  `FANGST_<SECTION>__<KEY>` variables, then flags. A layer only holds what it changes. The merged
  table deserializes into `Defaults` (tunables) and `Settings`. Flags that existed before map onto
  the same keys (`--torrent-port` is `torrent.listen_port`, `--webhook-url` is
  `notifiers.webhook.url`); `--enable` adds to `providers.enabled`. The usenet server from
  `--host` or from `FANGST_USENET_HOST` and its companions is kept apart from the file's list
  and tried first.
- **Locked values.** Anything the environment or a flag sets is listed in `locked` by
  `GET /api/v1/settings`. A `PUT` that changes one is refused with its name; one sent back
  unchanged leaves the file's own value in place.
- **Secrets are never in the file.** A `password` key in a server is refused at start, naming
  `password_env`. The file and flags may only name an environment variable (`password_env`,
  `token_env`, `webhook_url_env`). Values entered in the web UI are sealed with `Secrets` (0003)
  under `server:<id>:password`, `notify:webhook:token`, `notify:ntfy:token` and
  `notify:discord:webhook_url`. Servers get a stable `id` so a rename keeps the password; a
  removed server's password is deleted. The API never returns a secret, only `has_password` and
  the like; `null` in a request keeps the stored value and `""` removes it.
- **Settings API.** `GET` and `PUT /api/v1/settings`, for the owner (the first user) only. `PUT`
  replaces everything, builds the new servers and notifiers before storing anything, then writes
  secrets, the file (through a temporary file and a rename) and applies what can change while
  running: usenet servers (the next job uses them), notifiers, categories and their folders, and
  adapters (a switched-off adapter's paths answer 404). The rest waits for a restart and the
  response names it: the downloads folder, torrent and HTTP settings, post-processors and their
  tunables. `fangst-dl-api` defines the `Control` trait; `fangst-dl` implements it, since reading
  files and building providers is wiring.
- **Categories** in the file are created in `downloads.db` at start and on save; one removed from
  the file is deleted there unless a job still carries it. A category's `folder` (relative to the
  downloads folder, or absolute) is where its new jobs go.

## Alternatives
- **`toml_edit` to keep comments.** Rewriting through `toml` loses them; the file says so at the
  top. `toml_edit` is a new dependency, and the UI is the main editor. Revisit if people
  hand-edit the file often.
- **Settings in the database.** No file to merge, but nothing to edit or mount in a container,
  and the database would then hold the tunables too.
- **Secrets in the file, encrypted.** A file with ciphertext still gets shared, and it needs the
  master key to be readable by hand anyway.
- **A generic env mapping for the usenet server.** `servers` is an array, which a flat variable
  name can't address well; `FANGST_USENET_HOST` and friends cover the one server a container
  usually needs.

## Consequences
- Every tunable is settable from the environment without code, which Docker and Helm use.
- Hand-written comments in `fangst.toml` disappear the first time the UI saves.
- Text values that look like numbers need quoting in variables (`'"123"'`).
- A secret set through an environment variable is shown as present but can't be replaced from
  the web UI; the variable wins.
- A running usenet job keeps the servers it started with until it finishes.
