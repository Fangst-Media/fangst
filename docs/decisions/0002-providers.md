# 0002. Extension points are traits over plain data

Status: accepted
Date: 2026-11-21
Issue or PR: #17

## Context
Fangst needs many interchangeable parts: post-processors (repair, unpack, safety checks, scripts),
notifiers, API adapters for other products' protocols, and later indexers, metadata sources,
sidecar writers and media servers. Milestone 3 adds plugins compiled to WebAssembly, which can
only exchange data, not Rust objects.

## Decision
- Every extension point is a trait in `fangst-providers` (layer 0, MIT/Apache), so anyone can
  implement it. Each built-in implementation lives in its own crate.
- Trait methods take and return serde types only. No references into Fangst internals, no Rust
  handles, no callbacks. An adapter gets a `Jobs` trait object for the operations it translates onto.
- Methods return `Pin<Box<dyn Future + Send>>` rather than `async fn`, so the traits work as
  `dyn Trait` in a registry.
- Every provider has a stable `id`. Nothing is on unless enabled by id (`fangst-dl serve --enable`,
  later the config file). An unknown id stops startup and lists the available ones.
- Post-processors run in registry order; the first `Fail` ends the chain and fails the job.
  A notifier failure never fails a job.

## Alternatives
- `async fn` in traits: cleaner to write, but not usable as `dyn Trait` without extra machinery.
- An `async-trait` style macro: hides the boxing, but adds a dependency for little gain.
- Passing engine or database handles to providers: convenient for built-ins, impossible for
  WebAssembly plugins, and it would let providers reach around the service layer.

## Consequences
- A WebAssembly host (#57) can implement each trait by serializing the same types to its WIT
  interface.
- Built-in providers can't take shortcuts into internals; anything they need must exist as data or
  as a `Jobs` operation, which also makes it available to the native API.
- Boxed futures cost one allocation per call, negligible next to network and disk work.
