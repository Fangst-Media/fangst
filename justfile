# Task runner for the Rust and web sides. AGENTS.md lists every command.

# PowerShell exists on every Windows install; Git's sh is not always on PATH.
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

# Install JS dependencies and fetch Rust crates
setup:
    bun install
    cargo fetch
    bun run --cwd web e2e:install

# Fast gate: run while coding
check: fmt-check clippy test ts-check

# Full gate: run before every commit and merge
check-full: check web-build layercheck deny generated web-test changeset

# Layer and layer-0 licence rules from AGENTS.md
layercheck:
    cargo run --quiet -p layercheck

# Advisories, licences, bans and sources for every dependency
deny:
    cargo deny check

# Fail when a committed generated file no longer matches the code that generates it
generated:
    cargo run --quiet -p fangst-dl -- openapi --check openapi/dl.json

# Rewrite the committed OpenAPI documents after an API change
openapi:
    cargo run --quiet -p fangst-dl -- openapi openapi/dl.json

# Fail when the branch adds no changeset; passes on main and when nothing changed
changeset:
    changesetter check --base origin/main

# Usenet benchmark against the mock server; compares with this machine's baseline in bench/
bench-usenet *ARGS:
    cargo run --release --quiet -p usenet-bench -- {{ARGS}}

# Record this machine's baseline for the default benchmark scenario
bench-baseline:
    cargo run --release --quiet -p usenet-bench -- --save-baseline

# Sonarr, Radarr and HolyArrs against the adapters in Docker (sabnzbd, qbittorrent, holyarrs or all).
# Not part of the gate; needs Docker and internet for the images and Sonarr/Radarr metadata.
test-adapters SUITE="all" *ARGS:
    bun x tsc -p tests/adapters
    bun run tests/adapters/run.ts {{SUITE}} {{ARGS}}

# Turn the last adapter run's recordings into fixtures/adapters/*/recorded
record-adapter-fixtures:
    bun run tests/adapters/fixtures.ts

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

# Production build of the web app; catches config and bundling errors the typecheck can't
web-build:
    bun run --cwd web build

# Component tests, then the browser smoke test against a real fangst-dl and Vite server
web-test:
    bun run --cwd web test
    bun run --cwd web test:e2e

# Run the web app with hot refresh against the old TypeScript API on :5001
web-dev:
    bun run --cwd web dev

# Typecheck and lint the TypeScript side
ts-check:
    bun install --frozen-lockfile
    bun run typecheck
    bun run lint
