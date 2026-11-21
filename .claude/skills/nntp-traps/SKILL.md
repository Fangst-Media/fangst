---
name: nntp-traps
description: Known traps in NNTP servers, yEnc encoding and NZB files that real usenet providers and posters hit. Read before changing fangst-nntp, fangst-yenc, fangst-nzb or fangst-usenet.
---

# NNTP, yEnc and NZB traps

Real servers and posters break the specs in ways tests against a clean mock server won't show. Every trap found while building Fangst goes here, with the symptom, the cause and where the handling lives in code.

Format for each entry:

```markdown
## Short name of the trap

**Symptom:** what you see.
**Cause:** what the server, poster or file actually does.
**Handled in:** `crates/<crate>/src/<file>.rs`, and the test that covers it.
```

## yEnc line ending in a lone `=`

**Symptom:** a decoder reads past the line end or panics on the last byte of a line.
**Cause:** some encoders split an escape sequence across lines, leaving `=` as the last byte.
**Handled in:** `crates/fangst-yenc/src/lib.rs` (`decode_line` drops the dangling `=`; the CRC check then reports the damage). Covered by the `arbitrary_text_after_header_never_panics` property test.

## Dot-stuffing is not yEnc's job

**Symptom:** decoded data has an extra `.` at the start of some lines.
**Cause:** NNTP doubles a leading `.` on every line of a multi-line response. The NNTP client must undo it before yEnc decoding sees the body.
**Handled in:** `fangst-nntp` (#10). `fangst-yenc` assumes it is already done.

## Host resolves to an address that doesn't answer

**Symptom:** connecting to a provider (or `localhost` in tests) fails or takes about 2 seconds on Windows, while the same host works in other tools.
**Cause:** the name resolves to several addresses, often IPv6 first, and only some of them are reachable from this network. Trying only the first one fails.
**Handled in:** `crates/fangst-nntp/src/lib.rs` (`Connection::connect` tries every resolved address in order). Covered by the TLS tests, which connect to `localhost` while the mock server listens on IPv4 only.

## The PAR2 index file says repair is impossible

**Symptom:** verifying against the small `.par2` index reports damage with zero recovery blocks available, so `rust_par2::VerifyResult::repair_possible` is `false` even though the post has enough recovery volumes.
**Cause:** posters put only checksums in the index; recovery blocks live in `<base>.volNN+MM.par2` files, which the verifier never sees until they are downloaded.
**Handled in:** `crates/fangst-postproc-par2/src/lib.rs` (`choose_volumes` plans from the volume names, `repair` counts the real blocks on disk) and `crates/fangst-downloads/src/pipeline.rs`. Covered by `par2_five_percent_missing_articles_are_repaired` in `crates/fangst-downloads/tests/par2.rs`.

## A recovery volume holds fewer blocks than its name says

**Symptom:** after fetching volumes whose names add up to the blocks needed, repair still reports too few.
**Cause:** the volume itself lost articles; every recovery packet a missing article touches fails its MD5 and is skipped.
**Handled in:** `crates/fangst-downloads/src/pipeline.rs` (`repair_set` takes the real count from the failed repair and fetches more volumes for the shortfall). Covered by `par2_damaged_recovery_volume_fetches_more`.

## PAR2 file names point outside the job folder

**Symptom:** a repair could create or overwrite a file outside the job folder.
**Cause:** file names in PAR2 FileDesc packets are untrusted, and `rust-par2` joins them onto the folder as given, including `../` and absolute paths.
**Handled in:** `crates/fangst-postproc-par2/src/lib.rs` (`load` refuses any name that isn't a plain safe file name before verify or repair touch the disk). Covered by `names_that_leave_the_folder_are_refused`.

## Archive entry names point outside the job folder

**Symptom:** extracting a RAR or 7z writes a file next to or above the job folder, or replaces one there.
**Cause:** entry names are untrusted. Posters (and attackers) use `../`, a leading `/` or `\`, a drive (`C:\`), names Windows treats as devices (`CON.mkv`), and symlink entries whose target leads out. Archive libraries clean some of these and pass others through as given.
**Handled in:** `crates/fangst-postproc-extract/src/names.rs` (`entry_path` refuses instead of rewriting; `check_link`). The extractor applies it to every entry even with `postproc-safety` off and never creates links. Covered by `extract_refuses_unsafe_entries_on_its_own` and `safety_refuses_paths_and_links_leaving_the_job_folder` in `crates/fangst-postproc-extract/tests/archives.rs`.

## A link's target is only known after decoding

**Symptom:** a symlink entry passes a listing-only check and then points anywhere.
**Cause:** RAR5 keeps a link's target in the header, but RAR4 (Unix symlinks) and 7z keep it in the entry's data, so it can't be checked without decoding the archive.
**Handled in:** `crates/fangst-postproc-extract/src/{rar,sevenz}.rs` mark such entries `LinkTarget::Unknown`, which both providers refuse. Covered by the `malicious/symlink.7z` cases.

## RAR volumes come in two naming schemes

**Symptom:** only the first part of a multi-volume RAR is found, or volumes sort as `part1, part10, part2`.
**Cause:** current posts use `name.part01.rar`, `name.part02.rar`; older ones `name.rar`, `name.r00`, `name.r01` and so on, sometimes past `.r99`. 7-Zip splits into `name.7z.001`, `name.7z.002` at arbitrary byte offsets.
**Handled in:** `crates/fangst-postproc-extract/src/sets.rs` (`find`, ordered by the number, not the text). Covered by its unit tests and the `rar4-old-naming` and `7z-volumes` fixtures.

## The RAR reader needs a bigger stack than a Windows thread has

**Symptom:** `thread has overflowed its stack` while handling an encrypted RAR in a debug build on Windows.
**Cause:** rar-rs keeps large decoder and key state on the stack; Windows gives a main thread 1 MiB.
**Handled in:** `crates/fangst-postproc-extract/src/lib.rs` (`on_worker` runs every archive step on its own thread with an 8 MiB stack). The fixture generator in `fixtures/archives/generate` does the same.
