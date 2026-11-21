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
