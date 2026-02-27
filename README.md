# crux

Compress CLI output for AI agents. Save 60-90% tokens. Zero config.

## What it does

AI coding agents run shell commands and feed the output back into the LLM context window. Most of that output is noise — progress bars, hints, ANSI colors, repeated blank lines. crux strips all of it, keeping only what the AI needs.

```sh
# Without crux: git status → 47 lines, 1,800 tokens
# With crux:    git status → 6 lines, 180 tokens (90% savings)

crux run git status
# On branch main
# Changes not staged for commit:
#   modified:   src/main.rs
#   modified:   src/lib.rs
```

## Install

```sh
cargo install crux-cli
```

No config needed. Works out of the box with 50+ built-in filters.

## Quick start

```sh
# 1. Install the hook (one time)
crux init              # local project
crux init --global     # all projects

# 2. That's it — Claude Code now auto-compresses all command output

# Or run commands manually:
crux run cargo test
crux run git diff
crux run docker ps
```

## Built-in filters (50+)

Zero-config compressed output for common developer tools:

| Category | Commands |
|----------|----------|
| **Git** | status, diff, log, show, branch, commit, add, fetch, pull, push, stash |
| **Rust** | cargo build, test, clippy, check, fmt, install |
| **JavaScript** | npm install/test/build, tsc, eslint, prettier, jest, vitest, next build |
| **Python** | pytest, pip install, ruff, ruff check |
| **Go** | go build, go test, golangci-lint |
| **Docker** | ps, images, logs, compose |
| **GitHub CLI** | gh pr list/view/checks, issue list, run list, api |
| **Infrastructure** | kubectl, terraform plan, helm, make |
| **Package managers** | npm, yarn, pnpm, pip |
| **Utilities** | ls, find, grep, tree, cat, curl, wget, wc |

```sh
# See all filters
crux ls

# Check which filter matches a command
crux which git status
# → builtin: git status

# See filter details
crux show "git status"
```

## How it works

```
Command → Execute → Filter → Compressed Output
                      │
           ┌──────────┴──────────┐
      Builtin filters       TOML filters
     (compiled Rust)       (declarative)
```

1. **Builtins** — Compiled Rust functions that understand command output structure. Fast, smart compression.
2. **TOML filters** — Declarative config files for line-level filtering (skip/keep patterns, regex replace, section extraction).
3. **Priority** — Local TOML > global TOML > embedded stdlib > builtins. Override anything.

## TOML filter pipeline

For commands without a builtin, TOML filters provide a 12-stage pipeline:

```toml
command = "terraform plan"
description = "Keep resource changes and summary"
strip_ansi = true

keep = [
    "^\\s*[#~+-]",
    "^Plan:",
    "^No changes",
    "^Error:",
]

collapse_blank_lines = true
trim_trailing_whitespace = true
```

Pipeline stages (in order):
1. `match_output` — Short-circuit on output content match
2. `strip_ansi` — Remove ANSI escape codes
3. `replace` — Regex substitution
4. `skip` / `keep` — Line-level regex filtering
5. `section` — Extract sections between markers
6. `extract` — First regex match with template output
7. `dedup` — Collapse consecutive duplicate lines
8. `template` — Variable interpolation
9. `trim_trailing_whitespace`
10. `collapse_blank_lines`

## CLI commands

```sh
crux run <cmd>          # Run command through filter pipeline
crux err <cmd>          # Keep only error/warning lines
crux test <cmd>         # Extract test summary (auto-detect framework)
crux log <cmd>          # Run with dedup + collapse filters

crux ls                 # List all available filters
crux which <cmd>        # Show which filter matches
crux show <filter>      # Show filter config details
crux eject <filter>     # Export builtin as TOML for customization

crux init               # Install Claude Code hook (local)
crux init --global      # Install Claude Code hook (global)

crux gain               # Show total token savings
crux history            # Show recent command history with savings
crux verify             # Run declarative filter test suites
```

## Custom filters

Override any builtin or add new filters:

```sh
# Project-local: .crux/filters/
# Global: ~/.config/crux/filters/

mkdir -p .crux/filters
cat > .crux/filters/my-tool.toml << 'EOF'
command = "my-tool"
description = "Filter my-tool output"
strip_ansi = true

skip = [
    "^\\[debug\\]",
    "^\\s*$",
]

keep = [
    "^ERROR",
    "^WARN",
    "^Result:",
]

collapse_blank_lines = true
EOF
```

Eject a builtin to customize it:

```sh
crux eject "git status" > .crux/filters/git-status.toml
# Edit as needed — local TOML takes priority over builtin
```

## Agent integration

### Claude Code

```sh
crux init --global
# Adds hook to ~/.claude/settings.json
# All command output is now auto-compressed
```

### Manual hook setup

Add to your agent's command wrapper:

```json
{
  "hooks": {
    "Bash": {
      "command_output": "crux run"
    }
  }
}
```

## Optional features

```sh
# Build with all features
cargo build --features "lua,cache"
```

| Feature | Description |
|---------|-------------|
| `lua` | Lua escape hatch for complex filters (sandboxed mlua) |
| `cache` | rkyv-serialized filter discovery cache for faster startup |
| `tracking` | SQLite analytics for token savings (enabled by default in CLI) |

## Architecture

4-crate Rust workspace:

- **crux-core** — Filter engine, config loading, command runner
- **crux-cli** — CLI binary (clap)
- **crux-hook** — Agent hooks (Claude Code, Codex)
- **crux-tracking** — SQLite analytics

## License

MIT OR Apache-2.0
