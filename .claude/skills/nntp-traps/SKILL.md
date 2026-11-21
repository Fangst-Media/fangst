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

No entries yet.
