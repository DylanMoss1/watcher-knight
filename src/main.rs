use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::process;

use clap::{Parser, Subcommand};
use git2::Repository;
use walkdir::WalkDir;

const COMMENT_PREFIXES: &[&str] = &["//", "#", "--", "%", ";"];

#[derive(Parser)]
#[command(name = "watcher-knight")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan the repository for watcher-knight markers and validate them
    Run,
}

struct Marker {
    rel_path: String,
    line: usize,
    instruction: String,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Run => run(),
    }
}

fn run() {
    let repo = Repository::discover(".").unwrap_or_else(|_| {
        eprintln!("Error: not inside a git repository");
        process::exit(1);
    });
    let root = repo.workdir().unwrap_or_else(|| {
        eprintln!("Error: repository has no working directory");
        process::exit(1);
    });

    let mut markers = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
    {
        let entry = match entry {
            Ok(e) if e.file_type().is_file() => e,
            _ => continue,
        };
        let contents = match fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let rel_path = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();
        markers.extend(parse_markers(&contents, &rel_path));
    }

    if markers.is_empty() {
        eprintln!("No watcher-knight invariants found.");
        return;
    }

    let diff = git_diff(root);
    if diff.trim().is_empty() {
        eprintln!("No changes since HEAD^. Nothing to validate.");
        return;
    }

    pipe_to_claude(&build_prompt(&markers, &diff));
}

// ---------------------------------------------------------------------------
// Git diff
// ---------------------------------------------------------------------------

fn git_diff(root: &std::path::Path) -> String {
    let output = process::Command::new("git")
        .args(["diff", "HEAD^"])
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to run `git diff HEAD^`: {e}");
            process::exit(1);
        });
    if !output.status.success() {
        eprintln!(
            "Error: `git diff HEAD^` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        process::exit(1);
    }
    String::from_utf8_lossy(&output.stdout).to_string()
}

// ---------------------------------------------------------------------------
// Marker parsing
// ---------------------------------------------------------------------------

fn strip_comment_prefix<'a>(
    line: &'a str,
    expect: Option<&str>,
) -> Option<(&'a str, &'static str)> {
    let trimmed = line.trim_start();
    let candidates: &[&str] = match expect {
        Some(e) => {
            if let Some(rest) = trimmed.strip_prefix(e) {
                for &pfx in COMMENT_PREFIXES {
                    if pfx == e {
                        return Some((rest, pfx));
                    }
                }
            }
            return None;
        }
        None => COMMENT_PREFIXES,
    };
    for &pfx in candidates {
        if let Some(rest) = trimmed.strip_prefix(pfx) {
            return Some((rest, pfx));
        }
    }
    None
}

fn parse_markers(contents: &str, rel_path: &str) -> Vec<Marker> {
    let mut markers = Vec::new();
    let lines: Vec<&str> = contents.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let (after_prefix, prefix) = match strip_comment_prefix(lines[i], None) {
            Some(pair) => pair,
            None => { i += 1; continue; }
        };
        if !after_prefix.trim_start().starts_with("<watcher-knight") {
            i += 1;
            continue;
        }

        let start_line = i + 1;
        let after_tag = after_prefix.trim_start().strip_prefix("<watcher-knight").unwrap();

        // Single-line: `// <watcher-knight some instruction />`
        if let Some(before_close) = after_tag.strip_suffix("/>") {
            let text = before_close.trim();
            markers.push(Marker {
                rel_path: rel_path.into(),
                line: start_line,
                instruction: text.to_string(),
            });
            i += 1;
            continue;
        }

        // Multi-line: collect body lines until `/>`.
        let mut body: Vec<&str> = Vec::new();
        i += 1;
        while i < lines.len() {
            let rest = match strip_comment_prefix(lines[i], Some(prefix)) {
                Some((r, _)) => r,
                None => break,
            };
            let trimmed = rest.trim();
            if trimmed.contains("/>") {
                if let Some(before) = trimmed.strip_suffix("/>") {
                    let t = before.trim();
                    if !t.is_empty() {
                        body.push(t);
                    }
                }
                i += 1;
                markers.push(Marker {
                    rel_path: rel_path.into(),
                    line: start_line,
                    instruction: body.iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("\n"),
                });
                break;
            }
            body.push(trimmed);
            i += 1;
        }
    }
    markers
}

// ---------------------------------------------------------------------------
// Prompt building & Claude invocation
// ---------------------------------------------------------------------------

fn build_prompt(markers: &[Marker], diff: &str) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "The following watcher-knight invariants were found in this repository. \
         The diff below shows the changes between HEAD^ and the current working tree.\n\
         \n\
         Your task: for each invariant, spawn a sonnet agent (model: \"sonnet\") to validate it \
         **inductively against the diff**. Each agent should:\n\
         1. Assume the invariant held at HEAD^ (even if it can't verify this).\n\
         2. Examine the diff to determine whether the changes could have broken the invariant.\n\
         3. If needed, use Read/Grep/Glob to inspect the current file contents for more context.\n\
         4. Return one of the following JSON responses:\n\
         \n\
         - {{ \"type\": \"response\", \"is_valid\": true }}\n\
           The invariant still holds after the changes.\n\
         \n\
         - {{ \"type\": \"response\", \"is_valid\": false, \"reason\": \"...\" }}\n\
           The changes broke the invariant. Explain why.\n\
         \n\
         - {{ \"type\": \"malformed\", \"reason\": \"...\" }}\n\
           The invariant itself is no longer applicable — e.g. a referenced file was deleted, \
         or the code it describes has changed so drastically that the invariant no longer \
         makes sense. This is NOT for invariant violations; it is only for cases where \
         the watcher-knight marker needs to be rewritten or removed.\n\
         \n\
         If the diff does not touch anything relevant to an invariant, it is valid.\n\
         \n\
         After all agents have returned, check their results. \
         If every single response has {{ \"type\": \"response\", \"is_valid\": true }}, \
         output ONLY the exact text: All checks pass!\n\
         Otherwise, list each failing or malformed invariant with its details."
    ).unwrap();

    writeln!(out).unwrap();
    writeln!(out, "## Diff (HEAD^ → working tree)").unwrap();
    writeln!(out, "```diff").unwrap();
    write!(out, "{diff}").unwrap();
    if !diff.ends_with('\n') {
        writeln!(out).unwrap();
    }
    writeln!(out, "```").unwrap();

    writeln!(out).unwrap();
    writeln!(out, "## Invariants").unwrap();
    for (idx, m) in markers.iter().enumerate() {
        writeln!(out).unwrap();
        writeln!(out, "---").unwrap();
        writeln!(out, "Invariant {}", idx + 1).unwrap();
        writeln!(out, "File: {} (line {})", m.rel_path, m.line).unwrap();
        writeln!(out, "Instruction: {}", m.instruction).unwrap();
    }
    writeln!(out, "---").unwrap();
    out
}

fn pipe_to_claude(prompt: &str) {
    let mut child = process::Command::new("claude")
        .args([
            "-p",
            "--permission-mode", "dontAsk",
            "--allowedTools", "Task,Read,Grep,Glob",
            "--verbose",
        ])
        .env_remove("CLAUDECODE")
        .stdin(process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to launch `claude`: {e}");
            eprintln!("Make sure Claude Code is installed and `claude` is on your PATH.");
            process::exit(1);
        });

    child
        .stdin
        .take()
        .unwrap()
        .write_all(prompt.as_bytes())
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to write to claude stdin: {e}");
            process::exit(1);
        });

    let status = child.wait().unwrap_or_else(|e| {
        eprintln!("Error: failed to wait on claude process: {e}");
        process::exit(1);
    });
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
}
