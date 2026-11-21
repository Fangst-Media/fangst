@AGENTS.md

## Claude Code

The rules above are shared by every agent. Claude Code specifics:

- The playbooks in `.claude/skills/` load as the `nntp-traps` and `arr-adapter-quirks` skills; use them as skills.
- `.claude/rules/comments.md` holds the comment standard.
- The maintainer works with the Saturate/agents skills: `/spec`, `/plan`, `/build` (incremental-implementation plus tdd), `commit`, `make-pr`, `evaluating-dependencies`. Specs and plans live in `~/.claude/specs/Jaeger/fangst-platform/`, never in the repo.
