---
name: CLI failures in Claude Code sessions
description: lw CLI commands produce zero output when run from Claude Code bash tool — commands like lw db fresh hang silently
type: project
---

`lw` CLI commands (e.g., `lw db fresh`, `lw db migrate`) produce zero output and appear to hang when invoked from Claude Code's Bash tool. The underlying Docker commands work fine — `docker compose exec backend python manage.py makemigrations` and `docker compose exec backend python manage.py migrate_schemas` both succeed immediately.

**Why:** The `lw` binary (Go CLI at `~/go/bin/lw`) may be using interactive terminal features (TTY detection, spinners, color output) that don't work in Claude Code's non-interactive shell. Or the bash-guard hook may be interfering with execution.

**How to apply:** When `lw` commands fail silently in Claude Code sessions, fall back to the underlying Docker commands directly. This is a known issue — not a violation of CLI-first policy. File a GitHub issue to fix `lw` for non-TTY environments.
