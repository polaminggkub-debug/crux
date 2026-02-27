# crux

Compress CLI output for AI agents. Save 60-90% tokens. Zero config.

## What it does

Run any command through crux — get shorter output that AI still understands.

```sh
# before: git status dumps 50 lines
# after:
crux run git status
# → 5 lines, everything you need
```

## Install

```sh
cargo install crux-cli
```

That's it. No config needed. Works out of the box.

## Usage

```sh
# run commands through crux
crux run git diff
crux run cargo test
crux run npm test

# set up hook for Claude Code (one time)
crux init --global

# see how many tokens you've saved
crux gain
```

## Why?

- **Zero config** — install and go. 40+ built-in filters for common commands.
- **Customizable** — drop a simple TOML file to override any filter.
- **Fast** — Rust. Startup under 5ms.
- **Agent-ready** — hooks into Claude Code and Codex transparently.

## License

MIT OR Apache-2.0
