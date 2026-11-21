# Fangst

Self-hosted media platform in Rust. Milestone 1 is one download daemon for torrent and usenet that speaks the qBittorrent and SABnzbd APIs. Milestone 2 is a media manager on top that keeps files as they are, writes NFO and JSON sidecars, and serves a headless OpenAPI backend. The repo is mid-transition: the old Jæger TypeScript packages (`packages/api`, `packages/shared`, `packages/parser`) stay until milestone 2 replaces them.

This file is for every coding agent (Claude Code, Codex, Cursor, Copilot, Gemini CLI, Aider and the rest) and for people. Playbooks for known traps live in `.claude/skills/` (Claude Code loads them as skills; other agents read them as Markdown):

- `.claude/skills/nntp-traps/SKILL.md`: read before touching NNTP, yEnc, NZB or the usenet engine.
- `.claude/skills/arr-adapter-quirks/SKILL.md`: read before touching an API adapter.

## Pinned versions

- Rust 1.95.0, pinned in `rust-toolchain.toml`.
- Dev tools: `just`, `cargo-nextest`, `cargo-deny`, `changesetter`, Bun.
- Bun for the JS side. The web app in `web/` is React 19, TanStack Router and Query, Tailwind 3, Vite, TypeScript strict.

When an API detail of a crate or upstream protocol is uncertain, check its docs or source instead of guessing, and say so.

## Commands

Commands marked "planned" arrive with the issue named; until then they do not exist.

| What | Command |
| --- | --- |
| First setup | `just setup` |
| Fast gate (while coding) | `just check`: fmt, clippy, tests, TypeScript typecheck and Biome lint |
| Full gate (before every commit and merge) | `just check-full`: adds `layercheck`, `cargo deny`, regenerate-and-diff (empty so far), changeset check |
| Layer rules only | `just layercheck` (layer map in `layers.toml`) |
| Run the download daemon | `FANGST_API_KEY=<16+ chars> cargo run -p fangst-dl -- serve --data-dir data --host news.example.com` (API on 127.0.0.1:7740; without `--host` jobs wait in the queue) |
| Update the OpenAPI document | `just openapi` after changing an API route or type; `openapi/dl.json` is committed and the gate fails when it drifts |
| Tests | `just test` (`cargo nextest run --workspace`) |
| Add a changeset | `changesetter add --package fangst-core --bump patch --type fix -m "What changed, for users"` (`--no-bump` for tooling, docs and tests; `--bump minor` for new features). Packages share one version, so any package in the group works |
| Web app | `just web-dev` (Vite with hot refresh on :5173, proxies `/api` to :5001), `just web-build` (production build into `web/dist`, part of the full gate) |
| Old TS stack | `bun run dev` (until milestone 2). Biome skips `packages/` and the generated `web/src/api/`; typecheck still covers them |

CI (`.github/workflows/check.yml`) runs `just check-full` on Linux, macOS and Windows for every push and for pull requests from forks. Run `just check-full` before committing.

## Architecture rules

1. **Parts, not a monolith.** Every crate sits on one layer and depends only on lower layers. Crates on the same layer never depend on each other, except layer 0 crates, which may depend on other layer 0 crates as long as there are no cycles. `tools/layercheck` enforces this from `layers.toml`; a new crate must be added there or the gate fails.

   | Layer | Crates | Rule |
   | --- | --- | --- |
   | 0. Formats and protocols | `fangst-nntp`, `fangst-yenc`, `fangst-nzb`, `fangst-release`, `fangst-providers` | MIT/Apache. No database, config files or HTTP server. No GPL or AGPL dependencies |
   | 1. Engines and providers | `fangst-usenet`, `fangst-torrent`, built-in provider crates (`fangst-notify-*`, `fangst-indexer-*`, `fangst-meta-*` and the like) | One protocol or one external service end to end. Storage and settings are passed in |
   | 2. Services | `fangst-downloads`, `fangst-manager` | Business logic. Each owns its own SQLite file and migrations. Never call each other directly |
   | 3. Interfaces | `fangst-dl-api`, `fangst-manager-api`, `fangst-adapter-*` | Thin HTTP layers over the public operations of one service |
   | 4. Binaries | `fangst-dl`, `fangst-manager-bin`, `fangst` | Wiring and config only |

   `fangst-core` (AGPL) holds shared plumbing (errors, config helpers, tracing, path safety, the defaults file) and no business logic. Layers 1 to 4 may depend on it; layer 0 crates may not, because they are MIT/Apache.
2. **Native API first, adapters at the edge.** Fangst's own API is designed for Fangst. qBittorrent, SABnzbd and Sonarr/Radarr compatibility live in `fangst-adapter-*` crates that only translate onto the public operations of one service. An adapter holds no business logic and no state beyond ID mapping. If an adapter needs something the native API can't express, fix the service so the native API gains it too.
3. **Providers for every extension point.** Traits live in `fangst-providers`; each built-in implementation is its own crate. Provider traits take and return plain serde data, never Rust handles or callbacks, so a WebAssembly component can implement them later.
4. **Separate storage.** Each service owns its own database file. No service reads another service's tables.
5. **Removable parts.** Every optional provider and adapter can be switched off in config. The full gate boots each binary with all of them off and fails if it doesn't come up healthy (`crates/fangst-dl/tests/serve.rs`).
6. **No magic numbers.** Tunables (connections, timeouts, backoff, seeding defaults) live in one defaults file, `crates/fangst-core/defaults.toml`, documented in `TUNING.md` at the repo root.
7. **Untrusted input at the edge.** NZB files, torrent metadata, adapter request parameters and file paths are validated where they enter. Paths stay inside configured roots; `..` and links that point outside are refused. Extraction never writes outside the job folder and blocks executables.

## Code style

- Rust: `cargo fmt`, `clippy::pedantic` with `-D warnings`, `unsafe` denied unless asked. `thiserror` in library crates, `anyhow` only in binaries.
- TypeScript: strict, no casts (narrow instead), no `any`, types only from the generated OpenAPI client.
- Comments explain why, never what. No em dashes in comments or docs.
- Released API paths, settings keys, table names and job names are never renamed, only added.
- Describe behaviour, not other products. "Like Sonarr does" means different things to different readers; write the rules, numbers and behaviour instead. Adapter crates are the exception, since they implement a named protocol.

## Workflow

- Work goes spec, then plan, then small vertical slices with tests first, then commit and PR. Specs and plans stay out of the repo; the issue holds what is agreed.
- GitHub issues are the task list, one milestone per phase. One branch per issue, named `issue-<n>-<slug>`; commits and PRs reference the issue.
- **Ideas are issues.** Anything outside the current issue becomes a new issue labeled `idea`, with no milestone. Don't build it until it has been triaged into a milestone.
- **Bugs found on the way.** A small fix inside files the current issue already touches may go in the same PR, named in the PR description. Anything else becomes a new issue labeled `bug`.
- Write a failing test first wherever behaviour is testable.
- Every branch adds a changeset in `.changeset/`; the full gate fails without one. Write it for users, as one line of what they will notice, not as a commit message. Never edit versions or `CHANGELOG.md` by hand, and never run `changesetter version` or `release` unless the maintainer asks. New shipped crates join the `fixed` group in `changesetter.toml`.
- **Fresh eyes.** UI changes are captured as screenshots and judged by a fresh agent that never saw the code. Adapter responses are compared against real responses recorded in `fixtures/adapters/<adapter>/`, captured from local containers with throwaway credentials (#22, #26).
- **Decision records.** Choices others will copy (job model, adapter boundaries, database ownership, provider traits) get a short record in `docs/decisions/`. Read the ones touching your area first.
- **Question the current state.** Before copying an existing pattern into a new place, ask whether it is right for this case. If a better way exists, say so: take it, or record why not and open an issue.
- **Agents say who they are.** Every issue, PR and comment an agent writes ends with `Made by: <model> in <tool>, <steered by a person | on its own>`, and carries the `by:agent` label (`by:human` when a person wrote it), one type label (`bug`, `enhancement`, `documentation`, `idea`) and one `area:*` label (`area:daemon`, `area:usenet`, `area:torrent`, `area:compat`, `area:api`, `area:manager`, `area:web`, `area:infra`, plus `perf` where it fits). Outside contributors can't set labels, so for them the `Made by:` line is what counts. Use the model name you actually are; if you don't know it, say so.
- **Clean-room references.** HolyArrs, Scryer, Weaver, udl and nzbfast are references for behaviour. Read their docs and tests, write notes in the issue, implement from the notes. Don't copy their code structure or names.
- Ask before: adding a dependency, changing the layer map or architecture rules, database schema changes after the first tagged release, changing CI or release workflows, changing adapter behaviour, `unsafe` Rust, publishing crates or images.
- Never: commit secrets or real usenet credentials (also not in fixtures), post to external repos without the maintainer's approval, rename or move user media files unless renaming is turned on, skip or delete a failing test without approval, add telemetry.
- After a task, report what changed, how it was tested, and what is rough.

## Agent tooling traps

- In some agent shells on Windows (seen with Claude Code's Bash tool), backslashes inside Bash heredocs (including `python -` and `node -e`) get collapsed. Write files that contain backslashes with the edit or write tools, not through the shell.

## Legal

The project is AGPL-3.0-or-later (`LICENSE`). Layer 0 crates are MIT OR Apache-2.0 so other projects can use them. Contributions come in under `CLA.md`; the name is covered by `TRADEMARKS.md`. Layer 0 crates take only permissive dependencies (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0). Layers 1 to 4 may also use GPL-3.0 and LGPL code, since it is compatible with AGPL-3.0 (`rarpar` is one). Ask before any other licence.
