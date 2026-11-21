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

# Typecheck and lint the TypeScript packages (fails on main until #66)
ts-check:
    bun run typecheck
    bun run lint
