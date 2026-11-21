# 0006. Archive extraction and its safety check

Status: accepted
Date: 2026-11-21
Issue or PR: #19, #20

## Context
The plan for #19 named rarpar for RAR, as a GPL-3.0 crate that layer 1 may use. The crate behind
it, `unrar-rs` (scryer-media/rarpar), turned out different on both counts: since 0.11 it is
Apache-2.0 plus the UnRAR licence restriction (no use for building a RAR compressor), which is not
GPL-3.0 and which the FSF counts as incompatible with the GPL family; versions before 0.11 were
GPL-3.0 with the same restriction. Every release also depends on `reedsolomon-rs` and
`ppmd-turbo`, which need Rust 1.97.1, while the workspace is pinned to 1.95.0.

## Decision
- One crate, `fangst-postproc-extract`, holds two providers: `postproc-safety` and
  `postproc-extract`. They share the archive readers and the name rules; two crates on layer 1
  can't share code without a third.
- RAR is read with `rar-rs` (BSD-2-Clause, pure Rust, RAR 1.5 to 7, volumes, both kinds of
  encryption, builds on 1.95). 7z is read with `sevenz-rust2`, the maintained fork of
  `sevenz-rust`. No licence is added to `deny.toml`.
- The safety check lists every archive and every plain file before anything is written. It fails
  on a blocked file type (`[safety] blocked_extensions`), an entry with `..`, a root or a drive, a
  name `fangst_core::path::safe_file_name` would change, and a link pointing outside the job
  folder or whose target is only known after decoding (RAR4 and 7z keep it in the data).
- The extractor applies the path, name and link rules itself, so it never writes outside the job
  folder even with the safety check off. It never creates links, writes each file under a
  temporary name until its checksum matched, removes what it wrote when a later archive fails, and
  deletes the archives only after all of them extracted (`[extract] delete_archives`).
- The password comes from a job name ending in `{{password}}`, otherwise from the NZB's
  `password` meta. A missing or wrong password fails the job with that reason.

## Alternatives
- `unrar-rs` as planned: blocked by the toolchain pin, and its licence needs a decision from the
  maintainer before Fangst can ship it.
- A separate crate per provider: duplicates the RAR and 7z listing code or needs a shared crate.
- Shelling out to `unrar` or `7z`: an external dependency per platform, and the non-free `unrar`.

## Consequences
- RAR support rests on a young crate with one maintainer. The RAR code is one file
  (`src/rar.rs`), so swapping the reader is contained.
- Revisit when the maintainer rules on the UnRAR restriction, or when the toolchain moves past
  1.97.1 and `unrar-rs`'s repair features (recovery records, `.rev` volumes) become worth having.
- Executable blocking only works with `postproc-safety` enabled. Since providers are opt-in, a
  setup that enables extraction alone extracts executables; making the safety check default-on
  is a registry change for the maintainer.

## Maintainer decision
`rar-rs` stays (permissive licence, builds on the pinned toolchain). `postproc-safety` is required:
`fangst-dl serve` enables it for every job regardless of `--enable`, because blocking executables
lives only there.
