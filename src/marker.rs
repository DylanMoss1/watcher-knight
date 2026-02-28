use std::path::{Path, PathBuf};

const COMMENT_PREFIXES: &[&str] = &["//", "#", "--", "%", ";"];

pub struct Marker {
    pub name: String,
    pub rel_path: String,
    pub line: usize,
    pub instruction: String,
    pub files: Vec<String>,
}

fn strip_comment_prefix<'a>(
    line: &'a str,
    expect: Option<&str>,
) -> Option<(&'a str, &'static str)> {
    let trimmed = line.trim_start();
    if let Some(e) = expect {
        if let Some(rest) = trimmed.strip_prefix(e) {
            for &pfx in COMMENT_PREFIXES {
                if pfx == e {
                    return Some((rest, pfx));
                }
            }
        }
        return None;
    }
    for &pfx in COMMENT_PREFIXES {
        if let Some(rest) = trimmed.strip_prefix(pfx) {
            return Some((rest, pfx));
        }
    }
    None
}

/// Extract the name from text after `<watcher-knight`.
/// Expects formats like `: some-name` or `: some-name />`.
fn extract_name(after_tag: &str) -> String {
    let s = after_tag.trim_start();
    let s = s.strip_prefix(':').unwrap_or(s).trim_start();
    // Take everything up to whitespace, newline, or `/>`.
    let end = s
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(s.len());
    s[..end].to_string()
}

/// Normalize a path by resolving `.` and `..` components without touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                components.pop();
            }
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// Parse `files = { ./a.ts, ./b.py }` entries from body lines.
/// Returns (resolved file paths, remaining body lines for instruction).
fn extract_files<'a>(
    body: &[&'a str],
    marker_parent: &Path,
    repo_root: &Path,
) -> (Vec<String>, Vec<&'a str>) {
    let mut files = Vec::new();
    let mut remaining = Vec::new();

    for &line in body {
        if let Some(inner) = parse_files_line(line) {
            for entry in inner.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                let joined = marker_parent.join(entry);
                let normalized = normalize_path(&joined);
                let pattern_str = normalized.to_string_lossy().to_string();

                // Try glob expansion against the repo root
                let abs_pattern = repo_root.join(&pattern_str);
                let abs_str = abs_pattern.to_string_lossy().to_string();
                match glob::glob(&abs_str) {
                    Ok(paths) => {
                        let mut matched = false;
                        for path_result in paths {
                            if let Ok(abs_path) = path_result {
                                if let Ok(rel) = abs_path.strip_prefix(repo_root) {
                                    files.push(rel.to_string_lossy().to_string());
                                    matched = true;
                                }
                            }
                        }
                        // If no matches (e.g. file doesn't exist yet), keep the literal pattern
                        if !matched {
                            files.push(pattern_str);
                        }
                    }
                    Err(_) => {
                        files.push(pattern_str);
                    }
                }
            }
        } else {
            remaining.push(line);
        }
    }

    (files, remaining)
}

/// Try to parse a line as `files = { ... }`. Returns the inner content if matched.
fn parse_files_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let after_files = trimmed.strip_prefix("files")?;
    let after_eq = after_files.trim_start().strip_prefix('=')?;
    let after_eq = after_eq.trim_start();
    let inner = after_eq.strip_prefix('{')?.strip_suffix('}')?;
    Some(inner.trim().to_string())
}

pub fn parse_markers(contents: &str, rel_path: &str, repo_root: &Path) -> Vec<Marker> {
    let mut markers = Vec::new();
    let lines: Vec<&str> = contents.lines().collect();
    let mut i = 0;

    let marker_parent = Path::new(rel_path).parent().unwrap_or(Path::new(""));

    while i < lines.len() {
        let (after_prefix, prefix) = match strip_comment_prefix(lines[i], None) {
            Some(pair) => pair,
            None => {
                i += 1;
                continue;
            }
        };
        if !after_prefix.trim_start().starts_with("<watcher-knight") {
            i += 1;
            continue;
        }

        let start_line = i + 1;
        let after_tag = after_prefix
            .trim_start()
            .strip_prefix("<watcher-knight")
            .unwrap();
        let name = extract_name(after_tag);

        // Single-line: `// <watcher-knight: name some instruction />`
        if let Some(before_close) = after_tag.strip_suffix("/>") {
            // Strip the name portion from the instruction text.
            let text = before_close.trim();
            let text = text.strip_prefix(':').unwrap_or(text).trim_start();
            let instruction = text.strip_prefix(&name).unwrap_or(text).trim().to_string();
            markers.push(Marker {
                name,
                rel_path: rel_path.into(),
                line: start_line,
                instruction,
                files: Vec::new(),
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

                let (files, remaining) = extract_files(&body, marker_parent, repo_root);
                markers.push(Marker {
                    name: name.clone(),
                    rel_path: rel_path.into(),
                    line: start_line,
                    instruction: remaining
                        .iter()
                        .filter(|s| !s.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join("\n"),
                    files,
                });
                break;
            }
            body.push(trimmed);
            i += 1;
        }
    }
    markers
}
