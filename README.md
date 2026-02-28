# watcher-knight

AI-powered code invariant validation. Define rules directly in your source code and watcher-knight enforces them on every change.

## What it does

watcher-knight scans your repository for inline markers — small annotations in comments that describe rules your code should follow. When you run it, each marker is validated against your current changes by an AI agent that reads the actual codebase to check whether the rule holds.

Think of it as assertions, but for cross-file concerns, architectural constraints, and integration contracts that traditional linters can't catch.

## Installation

```sh
cargo install --path .
```

Requires [Claude Code](https://docs.anthropic.com/en/docs/claude-code) to be installed and authenticated.

## Quick start

Add a marker anywhere in your code using a comment:

```ts
// <wk: api-contract [./frontend.ts, ./backend.py]
// Ensure the frontend API client matches the backend route definitions. />
```

Then run:

```sh
watcher-knight run
```

watcher-knight finds all markers, checks which ones are relevant to your current changes, and validates each one in parallel.

## Marker syntax

Markers use `<wk: name ... />` inside any standard comment style (`//`, `#`, `--`, `%`, `;`).

**Inline (single-line):**

```py
# <wk: only-one-db-connection Ensure only one database connection is opened. />
```

**Multi-line:**

```ts
// <wk: error-handling
// Ensure that all API calls in this file handle
// 4xx and 5xx status codes explicitly. />
```

**With file scope:**

Restrict a watcher to specific files so it only runs when those files change:

```ts
// <wk: api-alignment [./client.ts, ./server.py]
// Ensure the client and server API definitions stay in sync. />
```

Or using the `files` directive for longer lists:

```rs
// <wk: schema-sync
// files = { ./migrations/*.sql, ./models.rs }
// Ensure database migrations match the model definitions. />
```

File paths are relative to the marker's directory and support glob patterns.

## Usage

```
watcher-knight run [OPTIONS]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--model <MODEL>` | `haiku` | AI model to use (`haiku`, `sonnet`, `opus`) |
| `--commit <COMMIT>` | `HEAD` | Git ref to diff against |
| `--all` | | Run all watchers, not just those matching changed files |

### Examples

```sh
# Validate against uncommitted changes (default)
watcher-knight run

# Use a more capable model
watcher-knight run --model sonnet

# Validate all changes since a branch point
watcher-knight run --commit main

# Run every watcher regardless of what changed
watcher-knight run --all
```

## Output

Watchers run in parallel. Results are printed as they complete:

```
running 3 watchers

[1/3] api-contract... OK
[2/3] only-port-5000... OK
[3/3] error-handling... FAILED

---- RESULTS ----

watcher api-contract... OK
watcher only-port-5000... OK
watcher error-handling... FAILED

---- FAILURES ----

---- error-handling (src/client.ts:42) ----

The fetch call on line 58 does not handle 4xx responses.
Status codes 400-499 will fall through without error handling.

watcher-knight result: FAILED. 2 passed; 1 failed
```

The process exits with code 1 if any watcher fails, making it suitable for CI pipelines and pre-commit hooks.

## CI integration

Add to your CI pipeline to enforce invariants on every push:

```yaml
# GitHub Actions
- name: Validate code invariants
  run: watcher-knight run --commit ${{ github.event.before }}
```

## License

MIT
