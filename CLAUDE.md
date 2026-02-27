# crux — Development Guidelines

## What is crux?

crux compresses CLI output for AI agents. Zero-config built-in handlers + customizable TOML filters. Saves 60-90% tokens.

## Architecture

4-crate workspace:

- **crux-core** — Filter engine, config loading, command runner. Pure library.
- **crux-cli** — CLI binary (clap). Thin shell over crux-core.
- **crux-hook** — Agent hooks (Claude Code, Codex). Minimal deps for fast startup.
- **crux-tracking** — SQLite analytics. Feature-gated, optional.

### Dual-Track Filter System

```
Command → Resolution → Execute → Filter → Output
                                    │
                         ┌──────────┴──────────┐
                    TOML filters          Builtin filters
                  (declarative)          (compiled Rust fn)
```

- **Builtins**: Zero-config compiled handlers (git, cargo, npm, docker, etc.)
- **TOML**: Declarative filters for customization (backward-compatible with tokf)
- Priority: local TOML > global TOML > embedded stdlib > builtins

### Key Directories

```
crates/crux-core/src/config/     — Config loading, resolution, types
crates/crux-core/src/filter/     — Filter pipeline (skip, replace, section, etc.)
crates/crux-core/src/filter/builtin/ — Compiled handlers (git.rs, cargo.rs, etc.)
crates/crux-cli/src/             — CLI entry, subcommands
crates/crux-hook/src/            — Agent integration hooks
crates/crux-tracking/src/        — SQLite tracking
tests/fixtures/                  — Real command outputs for testing
```

## Commits

Conventional Commits strictly:

```
<type>(<scope>): <description>
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `ci`, `perf`
Scopes: `core`, `cli`, `hook`, `tracking`, `filter`, `config`, `builtin`

## Testing

- **Fixture-driven**: Save real command outputs in `tests/fixtures/`. Tests load fixtures, apply filters, assert on output.
- **Unit tests**: Each filter stage gets unit tests in its module.
- **Integration tests**: `tests/cli_*.rs` for end-to-end CLI behavior.
- **`crux verify`**: Declarative test suites in `_test/` directories next to filter TOMLs.
- Run `cargo test` before every commit. Tests must pass.

## Code Quality

- `cargo fmt` before every commit
- `cargo clippy --workspace --all-targets -- -D warnings` must pass
- Functions under 60 lines
- Files under 500 lines (soft), 700 lines (hard)
- No over-engineering. YAGNI.

## Design Decisions (Do Not Revisit)

- TOML for config
- Capture then process, not streaming
- First match wins for config resolution
- Passthrough on missing filter
- Exit code masking (default on)
- Builtins provide zero-config coverage; TOML overrides when needed
- `crux eject <filter>` generates TOML from builtin for smooth escalation

## Build & Run

```sh
cargo build
cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt -- --check
```
