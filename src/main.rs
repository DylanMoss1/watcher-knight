mod claude;
mod marker;
mod prompt;

use std::fs;
use std::process;

use clap::{Parser, Subcommand};
use git2::Repository;
use walkdir::WalkDir;

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
        markers.extend(marker::parse_markers(&contents, &rel_path, root));
    }

    if markers.is_empty() {
        eprintln!("No watchers found.");
        return;
    }

    let diff = git_diff(root);
    if diff.trim().is_empty() {
        eprintln!("No changes since HEAD. Nothing to validate.");
        return;
    }

    let changed_files = git_changed_files(root);
    markers.retain(|m| m.files.is_empty() || m.files.iter().any(|f| changed_files.contains(f)));

    if markers.is_empty() {
        eprintln!("No watchers matched the changed files.");
        return;
    }

    claude::run_watchers(&markers, &diff);
}

fn git_changed_files(root: &std::path::Path) -> Vec<String> {
    let output = process::Command::new("git")
        .args(["diff", "HEAD", "--name-only"])
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to run `git diff HEAD --name-only`: {e}");
            process::exit(1);
        });
    if !output.status.success() {
        eprintln!(
            "Error: `git diff HEAD --name-only` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        process::exit(1);
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect()
}

fn git_diff(root: &std::path::Path) -> String {
    let output = process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to run `git diff HEAD`: {e}");
            process::exit(1);
        });
    if !output.status.success() {
        eprintln!(
            "Error: `git diff HEAD` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        process::exit(1);
    }
    String::from_utf8_lossy(&output.stdout).to_string()
}
