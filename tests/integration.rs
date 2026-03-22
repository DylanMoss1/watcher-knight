use std::fs;
use std::path::Path;
use std::process::Command;

// ── CLI parsing (via binary invocation) ───────────────────────────────────────

#[test]
fn cli_help_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_watcher-knight"))
        .arg("--help")
        .output()
        .expect("failed to run binary");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("watcher-knight"));
}

#[test]
fn cli_run_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_watcher-knight"))
        .args(["run", "--help"])
        .output()
        .expect("failed to run binary");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--model"));
    assert!(stdout.contains("--diff"));
    assert!(stdout.contains("--no-cache"));
}

#[test]
fn cli_run_no_markers_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_watcher-knight"))
        .args(["run", dir.path().to_str().unwrap()])
        .output()
        .expect("failed to run binary");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No watchers found"),
        "stderr was: {stderr}"
    );
}

#[test]
fn cli_run_nonexistent_dir() {
    let output = Command::new(env!("CARGO_BIN_EXE_watcher-knight"))
        .args(["run", "/tmp/wk_nonexistent_dir_12345"])
        .output()
        .expect("failed to run binary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot resolve path"),
        "stderr was: {stderr}"
    );
}

#[test]
fn cli_run_file_not_dir() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("a_file.txt");
    fs::write(&file_path, "hello").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_watcher-knight"))
        .args(["run", file_path.to_str().unwrap()])
        .output()
        .expect("failed to run binary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a directory"),
        "stderr was: {stderr}"
    );
}

// ── Marker parsing from example files ─────────────────────────────────────────

#[test]
fn example_frontend_parses_markers() {
    let contents = fs::read_to_string("example/frontend.ts").expect("example/frontend.ts missing");
    let repo_root = Path::new(".");
    let (markers, _errors) =
        watcher_knight::marker::parse_markers(&contents, "example/frontend.ts", repo_root);
    // frontend.ts has a format-explanation comment that looks like a marker but
    // isn't valid — so we only check that real markers are found.
    assert!(
        markers.len() >= 2,
        "expected at least 2 markers in example/frontend.ts, got {}",
        markers.len()
    );
}

#[test]
fn example_backend_parses_without_errors() {
    let contents = fs::read_to_string("example/backend.py").expect("example/backend.py missing");
    let repo_root = Path::new(".");
    let (_markers, errors) =
        watcher_knight::marker::parse_markers(&contents, "example/backend.py", repo_root);
    assert!(
        errors.is_empty(),
        "parse errors in example/backend.py: {errors:?}"
    );
}
