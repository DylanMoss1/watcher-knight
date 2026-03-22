# Watcher Knight

[![Crates.io](https://img.shields.io/crates/v/watcher-knight)](https://crates.io/crates/watcher-knight)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)

Automatic code validation tool powered by LLMs.

Verify code properties that are difficult to reason about using only static analysis tools and tests.

## What It Does

`watcher-knight` scans your codebase for `<wk ... />` watchers.

For each watchers, it runs a Claude agent to check whether the property still holds.

Think of it as assertions, but for cross-file concerns, architectural constraints, and integration contracts that traditional linters / type checkers / test suites can't catch.

Features: 
- **Caching.** Cache previous results if `files-to-watch` do not change
- **Diff mode.** Run watchers against git diffs.
- **Per-watcher options.** Specify Claude models and permissions for each watcher.

## Example Usage

Add "watchers" anywhere in your codebase using the format:

```js
// <wk: <watcher-name> [<files-to-watch (relative to current dir)>]
// options={...}  <-- optional
// Code properties to validate />
```

For example (`examples/frontend.ts`):

```js
// -- EXAMPLE 1: Validating APIs --
// <wk: front-and-backend-api-align [./frontend.ts, ./backend.py]
// Ensure that the backend (backend.py) and frontend (frontend.ts) API definitions align />
//
// ^ This will fail: the API definitions do not align
// (The previous result will be cached unless ./frontend.ts or ./backend.py are updated)

class BackendAPI {

  // -- EXAMPLE 2: Verifying port constraints --
  // <wk: only-port-5000 [./**/*]
  // options={model="haiku"}
  // Check that this is the only service started on port 5000. />
  //
  // ^ This will pass: this is the only service on port 5000

  constructor(private baseUrl = "http://localhost:5000") { }

  // -- EXAMPLE 3: Updating README --
  // <wk: error-400-in-readme
  // `examples/README.md` should explain what happens when the server returns error code 400. />
  //
  // ^ This will fail: the check cannot be completed as examples/README.md does not exist

  async getUserData(name: string): Promise<UserData> {
    const res = await fetch(
      `${this.baseUrl}/get_user_data?name=${encodeURIComponent(name)}`,
    );
    if (!res.ok) {
      throw new Error(`Request failed: ${res.status} ${res.statusText}`);
    }
    return res.json();
  }
}
```

To run the watcher knight: 

![watcher-knight run output](https://raw.githubusercontent.com/DylanMoss1/watcher-knight/main/assets/watcher-knight-run.png)

## Options

### CLI Options

```
watcher-knight run [root] [--model <model>] [--diff [ref]] [--no-cache]
```

| Option | Default | Description |
|---|---|---|
| `root` | Git repo root (or cwd if not in a git repo) | Directory to scan for watchers|
| `--model <model>` | `sonnet` | AI model to use: `haiku`, `sonnet`, or `opus` |
| `--diff [ref]` | — | Run in diff mode against a git ref. If no ref is given, auto-detects `origin/main` or `origin/master` |
| `--no-cache` | — | Skip cache and re-validate all watchers |

### Watcher Options

Per-watcher options are set inside the watcher body using `options={...}` syntax:

```js
// <wk: my-watcher [./src/*.ts]
// options={model="haiku", tools="Read,Grep"}
// Instruction text here />
```

| Option | Default | Description |
|---|---|---|
| `model` | CLI `--model` value | Override the AI model for this specific watcher |
| `tools` | `Read,Grep,Glob` | Comma-separated list of Claude tools the watcher agent is allowed to use |

### Watcher File Scoping

The `[...]` file list controls which files a watcher watches:

- Paths are relative to the watcher's directory
- Glob patterns are supported (e.g. `./src/*.ts`, `./migrations/*.sql`)
- If no files specified, watchers are always re-run and results are never cached
- In `--diff` mode, only watchers whose scoped files appear in the diff are run

## Installation

```sh
cargo install --path .
```

Requires [Claude Code](https://docs.anthropic.com/en/docs/claude-code) to be installed and authenticated.
