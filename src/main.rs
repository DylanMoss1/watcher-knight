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
    Run {
        /// AI model to use [haiku, sonnet, opus]
        #[arg(long, default_value = "haiku")]
        model: String,

        /// Git commit to diff against
        #[arg(long, default_value = "HEAD")]
        commit: String,

        /// Run all watchers regardless of changed files
        #[arg(long)]
        all: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { model, commit, all } => run(&model, &commit, all),
    }
}

fn run(model: &str, commit: &str, all: bool) {
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

    let diff = git_diff(root, commit);
    if diff.trim().is_empty() {
        eprintln!("No changes since {commit}. Nothing to validate.");
        return;
    }

    if !all {
        let changed_files = git_changed_files(root, commit);
        markers.retain(|m| m.files.is_empty() || m.files.iter().any(|f| changed_files.contains(f)));

        if markers.is_empty() {
            eprintln!("No watchers matched the changed files.");
            return;
        }
    }

    warn_unstaged_files(root);
    claude::run_watchers(&markers, &diff, model);
}

fn warn_unstaged_files(root: &std::path::Path) {
    let output = process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(root)
        .output();
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return,
    };
    let lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| format!("  - {}", l.trim()))
        .collect();
    if lines.is_empty() {
        return;
    }
    eprintln!(
        "\x1b[33m[WARNING] new unstaged files:\n{}\x1b[0m\n",
        lines.join("\n")
    );
}

fn git_changed_files(root: &std::path::Path, commit: &str) -> Vec<String> {
    let output = process::Command::new("git")
        .args(["diff", commit, "--name-only"])
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to run `git diff {commit} --name-only`: {e}");
            process::exit(1);
        });
    if !output.status.success() {
        eprintln!(
            "Error: `git diff {commit} --name-only` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        process::exit(1);
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect()
}

fn git_diff(root: &std::path::Path, commit: &str) -> String {
    let output = process::Command::new("git")
        .args(["diff", commit])
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to run `git diff {commit}`: {e}");
            process::exit(1);
        });
    if !output.status.success() {
        eprintln!(
            "Error: `git diff {commit}` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        process::exit(1);
    }
    String::from_utf8_lossy(&output.stdout).to_string()
}
