# 0001. One job table for every protocol, runtime-checked SQL

Status: accepted
Date: 2026-11-21
Issue or PR: #12

## Context
The download queue needs to survive restarts, hold torrent and usenet jobs side by side, and stay
movable to its own process later. The spec asked for sqlx with compile-time checked queries.

## Decision
- `fangst-downloads` owns `downloads.db` alone. No other crate reads its tables.
- `jobs` holds what every protocol shares (name, category, priority, status, error, output path,
  sizes, timestamps). Protocol details live in their own tables (`usenet_jobs`,
  `usenet_segments_done`); torrents get theirs in #24. The queue, API and UI only see `jobs`.
- Each stored segment gets a row in `usenet_segments_done` right after its bytes are flushed, so a
  restart skips it. Jobs left `downloading` by a stopped process go back to `queued` on start.
- WAL mode with `synchronous = NORMAL`: a power cut can lose the last few segment records, which
  only means downloading those segments again.
- Before any migration runs on an existing file, the database is copied with `VACUUM INTO` to
  `downloads.db.before-<version>.bak`.
- Queries use sqlx's runtime API (`query`, `query_as` with `FromRow`), not the `query!` macros.
  SQL strings must be `&'static str`, which sqlx 0.9 enforces.

## Alternatives
- Compile-time `query!` macros: they need a live database or committed offline data
  (`cargo sqlx prepare`) at build time, and `sqlx-cli` in CI and on every dev machine. Worth
  revisiting once the schema settles; the gate's `generated` step is where `prepare --check` would go.
- One wide `jobs` table with nullable protocol columns: simpler queries, but every protocol leaks
  into the shared model and the API.

## Consequences
- Schema mistakes in SQL surface in tests, not at compile time, so every query needs a test path.
- Splitting the downloader into its own process later means moving one file.
- Per-segment inserts are fine at 1 Gbit/s; the 10 Gbit/s work (#43) may need batching.
