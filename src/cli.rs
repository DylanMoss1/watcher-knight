use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};
use walkdir::WalkDir;

use crate::cache;
use crate::claude;
use crate::marker;

#[derive(Parser)]
#[command(name = "watcher-knight")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scan the repository for watcher-knight markers and validate them
    Run {
        /// Directory to scan for markers (default: git repo root, or cwd)
        #[arg()]
        root: Option<PathBuf>,

        /// AI model to use [haiku, sonnet, opus]
        #[arg(long, default_value = "sonnet")]
        model: String,

        /// Use git diff mode. Optional ref to diff against (default: auto-detect origin/main or origin/master)
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        diff: Option<String>,

        /// Skip cache, force all watchers to run fresh
        #[arg(long)]
        no_cache: bool,
    },
}

pub fn run(model: &str, diff: Option<&str>, no_cache: bool, root_arg: Option<&Path>) {
    let root = resolve_root(root_arg);

    let mut markers = collect_markers(&root);
    if markers.is_empty() {
        eprintln!("No watchers found.");
        return;
    }

    if let Some(diff_ref) = diff {
        run_diff_mode(&root, &mut markers, diff_ref, model);
    } else {
        run_cache_mode(&root, &markers, model, no_cache);
    }
}

/// Determine the root directory to scan for markers.
///
/// If an explicit path is given, canonicalize and use it directly.
/// Otherwise fall back to the git repo root, then the current working directory.
fn resolve_root(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        match path.canonicalize() {
            Ok(p) if p.is_dir() => return p,
            Ok(p) => {
                eprintln!("Error: `{}` is not a directory", p.display(),);
                process::exit(1);
            }
            Err(e) => {
                eprintln!("Error: cannot resolve path `{}`: {e}", path.display());
                process::exit(1);
            }
        }
    }

    // Try git repo first, fall back to cwd.
    if let Ok(repo) = git2::Repository::discover(".")
        && let Some(workdir) = repo.workdir()
    {
        return workdir.to_path_buf();
    }
    std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("Error: cannot determine working directory: {e}");
        process::exit(1);
    })
}

fn collect_markers(root: &Path) -> Vec<marker::Marker> {
    let mut markers = Vec::new();
    let mut all_errors = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_entry(|e| {
        let name = e.file_name();
        name != ".git" && name != ".watcher_knight"
    }) {
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
        let (file_markers, file_errors) = marker::parse_markers(&contents, &rel_path, root);
        markers.extend(file_markers);
        all_errors.extend(file_errors);
    }
    for err in &all_errors {
        eprintln!("\x1b[33m[WARNING] {err}\x1b[0m");
    }
    markers
}

fn run_diff_mode(root: &Path, markers: &mut Vec<marker::Marker>, diff_ref: &str, model: &str) {
    let diff_ref = if diff_ref.is_empty() {
        resolve_diff_ref(root)
    } else {
        diff_ref.to_string()
    };

    let diff = git_diff(root, &diff_ref);
    if diff.trim().is_empty() {
        eprintln!("No changes since {diff_ref}. Nothing to validate.");
        return;
    }

    let changed_files = git_changed_files(root, &diff_ref);
    markers.retain(|m| m.files.is_empty() || m.files.iter().any(|f| changed_files.contains(f)));

    if markers.is_empty() {
        eprintln!("No watchers matched the changed files.");
        return;
    }

    warn_unstaged_files(root);
    let n = markers.len();
    eprintln!("running {n} watchers\n");
    let results = claude::run_watchers(markers, Some(&diff), model, n, 0);
    claude::print_results(&results);
}

fn run_cache_mode(root: &Path, markers: &[marker::Marker], model: &str, no_cache: bool) {
    let mut cache = if no_cache {
        cache::Cache::new()
    } else {
        cache::load_cache()
    };

    let n = markers.len();
    let mut to_run_indices: Vec<usize> = Vec::new();
    let mut cached_results: Vec<claude::WatcherResult> = Vec::new();
    let mut completed = 0;

    eprintln!("running {n} watchers\n");

    for (i, marker) in markers.iter().enumerate() {
        if no_cache {
            to_run_indices.push(i);
        } else if let Some(entry) = cache::check_cache(marker, &cache, root) {
            completed += 1;
            let status = if entry.is_valid {
                "\x1b[32mOK\x1b[0m"
            } else {
                "\x1b[31mFAILED\x1b[0m"
            };
            eprintln!(
                "[{completed}/{n}] {}... {status} \x1b[90m(cached)\x1b[0m",
                marker.name
            );
            cached_results.push(claude::WatcherResult {
                name: marker.name.clone(),
                location: format!("{}:{}", marker.rel_path, marker.line),
                is_valid: entry.is_valid,
                reason: entry.reason.clone(),
                cached: true,
            });
        } else {
            to_run_indices.push(i);
        }
    }

    let to_run: Vec<marker::Marker> = to_run_indices.iter().map(|&i| markers[i].clone()).collect();

    let fresh_results = if to_run.is_empty() && cached_results.is_empty() {
        Vec::new()
    } else {
        claude::run_watchers(&to_run, None, model, n, completed)
    };

    // Update cache with fresh results
    for (marker, result) in to_run.iter().zip(fresh_results.iter()) {
        let (key, entry) = cache::build_entry(marker, result, root);
        cache.insert(key, entry);
    }
    cache::save_cache(&cache);

    let mut all_results = cached_results;
    all_results.extend(fresh_results);
    claude::print_results(&all_results);
}

fn resolve_diff_ref(root: &Path) -> String {
    for candidate in ["origin/main", "origin/master"] {
        let output = process::Command::new("git")
            .args(["rev-parse", "--verify", candidate])
            .current_dir(root)
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .status();
        if let Ok(status) = output
            && status.success()
        {
            return candidate.to_string();
        }
    }
    eprintln!(
        "Error: could not find origin/main or origin/master. Pass a ref explicitly: --diff <ref>"
    );
    process::exit(1);
}

fn warn_unstaged_files(root: &Path) {
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

fn git_changed_files(root: &Path, commit: &str) -> Vec<String> {
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

fn git_diff(root: &Path, commit: &str) -> String {
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
