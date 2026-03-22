# CLAUDE.md

> **Keep this file up to date.** When functionality, commands, or project structure changes, update this file accordingly.

## Overview

watcher-knight is a Rust CLI that enforces code invariants using Claude AI agents. Developers define rules as inline comment markers (`<wk: name ... />`), and the tool validates them by spawning Claude processes that read the actual codebase. Built for cross-file concerns, architectural constraints, and integration contracts that traditional linters can't catch.

Requires [Claude Code](https://docs.anthropic.com/en/docs/claude-code) to be installed and authenticated.

## Build & Run

```bash
cargo build                     # Build
cargo check                     # Type-check
cargo run -- run                # Run validation (default: cache mode, sonnet model)
cargo install --path .          # Install locally
```

## CLI Options

```bash
watcher-knight run                        # Cache-based validation with sonnet (from git root or cwd)
watcher-knight run example/              # Scan a specific directory instead of the default root
watcher-knight run --model haiku          # Use different model (haiku/sonnet/opus)
watcher-knight run --diff                 # Diff mode against origin/main or origin/master
watcher-knight run --diff some-branch     # Diff mode against specific ref
watcher-knight run --no-cache             # Skip cache, re-validate all watchers
```

Exit code 1 if any watcher fails.

## Marker Syntax

```
// <wk: marker-name [file1, file2] instruction text />            // single-line
// <wk: marker-name [./a.ts, ./b.py]                              // multi-line
// options={model="haiku"}                                         // per-marker options (optional)
// instruction text />
```

- Tags: `<wk:` or `<watcher-knight:`
- Comment styles: `//`, `#`, `--`, `%`, `;`
- File scope `[...]` restricts which files trigger the watcher; paths are relative to the marker's directory, glob patterns supported
- `files = { ... }` can be used in the body for longer file lists
- `options={...}` sets per-marker options (e.g. model override)

## Project Structure

```
src/
  main.rs       Entry point → cli::run()
  cli.rs        CLI parsing (clap), orchestration, git integration
  marker.rs     Parses <wk: .../> markers from source comments
  claude.rs     Spawns claude CLI processes in parallel, parses JSON results
  cache.rs      Hash-based caching in .watcher_knight/cache.json
  prompt.rs     Builds AI validation prompts
example/
  frontend.ts   Example markers (cross-file validation, port constraints, README checks)
  backend.py    Example Flask backend for cross-file demo
```

## Architecture Notes

- **Parallel execution**: Each watcher runs in its own `std::thread`, results collected via `mpsc::channel`
- **Claude invocation**: Spawns `claude -p` with `--allowedTools Read,Grep,Glob` and `--permission-mode dontAsk`
- **Caching**: Keyed on `marker_name::file_path`, invalidated when marker instruction hash or watched file content hashes change. Unscoped watchers (no files) always re-run. Cache stored in `.watcher_knight/cache.json`
- **Diff mode**: Filters markers to only those whose scoped files appear in `git diff --name-only`
- **Rust edition 2024**, dependencies: clap 4, git2, glob, serde/serde_json, walkdir
