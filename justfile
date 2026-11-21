# Task runner for the Rust and web sides. AGENTS.md lists every command.

# PowerShell exists on every Windows install; Git's sh is not always on PATH.
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

# Install JS dependencies and fetch Rust crates
setup:
    bun install
    cargo fetch

# Fast gate: run while coding. ts-check joins once #66 is fixed.
check: fmt-check clippy test

# Full gate: run before every commit and merge
check-full: check web-build layercheck deny generated changeset

# Layer and layer-0 licence rules from AGENTS.md
layercheck:
    cargo run --quiet -p layercheck

# Advisories, licences, bans and sources for every dependency
deny:
    cargo deny check

# Regenerate generated files and fail if any changed; OpenAPI specs and clients join here as they appear
generated:
    @echo "generated: nothing to regenerate yet"

# Fail when the branch adds no changeset; passes on main and when nothing changed
changeset:
    changesetter check --base origin/main

# Format all Rust code
fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run all Rust tests
test:
    cargo nextest run --workspace --no-tests=pass

# Production build of the web app; catches broken imports and config even while #66 keeps typecheck out of the gate
web-build:
    bun install --frozen-lockfile
    bun run --cwd web build

# Run the web app with hot refresh against the old TypeScript API on :5001
web-dev:
    bun run --cwd web dev

# Typecheck and lint the TypeScript packages (fails on main until #66)
ts-check:
    bun run typecheck
    bun run lint
