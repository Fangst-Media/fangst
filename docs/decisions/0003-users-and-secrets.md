# 0003. Users, sessions and secrets in downloads.db under a master key file

Status: accepted
Date: 2026-11-21
Issue or PR: #21

## Context
Until now one API key from `FANGST_API_KEY` guarded the whole daemon. The web UI needs a password
login, tools need API keys, and usenet server passwords must be stored somewhere other than plain
text in the database. The daemon runs on home servers, often over plain HTTP on a LAN, with no
secret manager around.

## Decision
- **Store.** Users, API keys, sessions and secrets live in `downloads.db` (migration
  `0004_users.sql`), owned by `fangst-downloads` like the rest of the file. The tables don't refer
  to jobs, so they can move to their own file when a second service needs logins.
- **Master key.** 32 random bytes in `<data-dir>/master.key`, created on first start with mode
  0600 on Unix. It is written to a temporary file, flushed, then hard-linked into place, so a crash
  never leaves a short key and two processes starting at once end up with the same key. On
  Windows the file inherits the data folder's ACL. Two subkeys are derived with
  HMAC-SHA256: one encrypts, one computes lookup hashes. The key is never logged and its `Debug`
  output is redacted. Losing the file makes stored secrets and API keys unreadable; backups must
  include it, and should be stored apart from the database.
- **Secrets** (usenet passwords and the like) are sealed with XChaCha20-Poly1305 (random 24-byte
  nonce per value). The row name is the associated data, so a ciphertext copied to another row
  does not open.
- **Passwords** are Argon2id with the `argon2` crate's defaults (19 MiB, 2 passes, 1 lane), at
  least 12 characters, hashed off the async threads. At most one hash per core and never more
  than 4 run at once (76 MiB). Past that a login gets 503 with `Retry-After: 1` straight away
  instead of waiting in a queue, and the attempt does not count toward the rate limit.
- **API keys** belong to a user. Each row holds an HMAC lookup hash (for matching a request) and
  the sealed key (so its owner can be shown it again). `FANGST_API_KEY` is stored on every start
  as the first user's key named `environment`, replacing the previous one, so rotating the
  variable revokes the old key. When the key actually changes, the first user's sessions end
  too, since whoever held the old key may have opened them. A fresh database gets the first user
  `admin` without a password; `PUT /api/v1/auth/password` with that key sets one. Whoever holds
  the environment key can therefore claim that account, so the operator can take it back from the
  machine itself: `fangst-dl reset-password --data-dir <dir> --user <name>` reads the new password
  from `--password-env <VAR>` or standard input (never from an argument) and ends the user's
  sessions.
- **Sessions.** `POST /api/v1/auth/login` sets `fangst_session`, a 256-bit random token, as an
  `HttpOnly; SameSite=Strict; Path=/` cookie for 30 days (`Secure` with `--secure-cookie`). Only
  its SHA-256 is stored. `POST /api/v1/auth/logout` deletes it; a password change ends all of the
  user's sessions. `GET /api/v1/auth/me` answers for either kind of caller.
- **Cross-origin requests.** `SameSite=Strict` only stops other *sites*. Other origins on the same
  site, such as another port on the same host or a sibling subdomain (`other.lab.example.com`
  calling `fangst.lab.example.com`), still get the cookie attached. So a request authenticated by
  the cookie that changes state (any method but GET, HEAD and OPTIONS), and every handshake for
  the `/api/v1/events` WebSocket, must carry `Sec-Fetch-Site: same-origin` or an `Origin` whose
  host and port equal the request's `Host`. Anything else gets 403. API keys are never attached by
  a browser on its own, so key callers skip this check.
- **Login rate limit**, per client address (IPv6 per /64), in memory: 5 free failures, then the
  wait doubles from 1 s up to 15 min; forgotten after an hour without failures; at most 10,000
  addresses tracked. An attempt counts as failed when it starts, so parallel guesses get no
  extra tries. A correct password forgives only the failures from that address against that
  account name, so signing in to your own account doesn't reset guesses against someone else's.
  The same limit guards the current-password check. A wrong password, an unknown user and a user
  without a password give the same 401 body and cost one Argon2 verification each; the decoy hash
  for unknown users is made at start, so the first one costs the same as later ones.
- **Errors.** Database, file and crypto errors are written to the daemon's standard error; callers
  get a fixed 500 message that names no paths, tables or library internals.

## Alternatives
- **Separate `auth.db`.** Cleaner ownership, but a second pool and migration set for four tables.
  Revisit when the manager needs the same users.
- **Master key from an environment variable or OS keyring.** An env variable leaks through process
  listings and container inspection as easily as a file; keyrings don't exist on headless
  servers. A file next to the data is what other self-hosted apps do and what users can back up.
- **AES-256-GCM.** Equally good with AES-NI, but its 12-byte random nonce has a tighter collision
  bound and it is slower on ARM boards without AES instructions.
- **Signed stateless session tokens.** No database read per request, but logout and password
  changes can't revoke them before they expire.
- **Rate limit per username.** Lets anyone lock a known user out; per address does not.

## Consequences
- Every API request with a key or cookie costs one indexed SQLite read.
- Behind a reverse proxy every client shares the proxy's address and therefore one rate limit;
  trusting `X-Forwarded-For` needs a configured list of trusted proxies first.
- The rate limit resets on restart.
- The limits and session lifetime are constants in `fangst-dl-api` (`AuthSettings`,
  `LoginLimits`), not yet in `defaults.toml`, because nothing reads that file yet.
- Rotating the master key means re-sealing every secret and API key; there is no command for it
  yet.
