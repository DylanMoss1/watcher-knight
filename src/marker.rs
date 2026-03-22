use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use nom::bytes::complete::{tag, take_while, take_while1};
use nom::character::complete::{char, space0};
use nom::multi::separated_list0;
use nom::sequence::tuple;
use nom::IResult;

// ── Types ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

#[derive(Debug, Clone)]
pub struct Marker {
    pub name: String,
    pub rel_path: String,
    pub line: usize,
    pub instruction: String,
    pub files: Vec<String>,
    pub options: HashMap<String, String>,
}

// ── Constants ──────────────────────────────────────────────────────────────────

const COMMENT_PREFIXES: &[&str] = &["//", "#", "--", "%", ";"];

// Ordered longest-first so `<watcher-knight` is tried before `<wk`.
const TAG_PREFIXES: &[&str] = &["<watcher-knight", "<wk"];

// ── Phase 1: Tag Extraction ────────────────────────────────────────────────────

struct RawTag {
    /// Everything from the tag prefix up to (but not including) `/>`,
    /// with comment prefixes stripped from continuation lines.
    content: String,
    /// 1-based line number of the opening tag.
    line: usize,
}

/// Find `<wk` or `<watcher-knight` in a line. Returns `(byte_offset, prefix_str)`.
/// Only matches when the prefix is followed by `:` or whitespace (to avoid false
/// positives like `<wking>`).
fn find_tag_in_line(line: &str) -> Option<(usize, &'static str)> {
    for &prefix in TAG_PREFIXES {
        if let Some(pos) = line.find(prefix) {
            let after = &line[pos + prefix.len()..];
            let next = after.chars().next();
            if next.is_none() || next == Some(':') || next.unwrap().is_whitespace() {
                return Some((pos, prefix));
            }
        }
    }
    None
}

/// Detect which comment prefix appears in the text before the tag.
fn detect_comment_prefix(before_tag: &str) -> Option<&'static str> {
    let trimmed = before_tag.trim();
    for &prefix in COMMENT_PREFIXES {
        if trimmed == prefix || trimmed.ends_with(prefix) {
            return Some(prefix);
        }
    }
    None
}

/// Strip a comment prefix from a continuation line. Returns `None` if a comment
/// prefix was expected but not found (i.e. the comment block ended).
fn strip_continuation<'a>(line: &'a str, comment_prefix: Option<&str>) -> Option<&'a str> {
    let trimmed = line.trim_start();
    match comment_prefix {
        Some(cp) => trimmed.strip_prefix(cp),
        None => Some(trimmed),
    }
}

/// Walk through the file contents, find every `<wk .../>` or `<watcher-knight .../>`
/// span, and return the raw tag content with comment prefixes stripped.
fn extract_raw_tags(contents: &str, file: &str) -> (Vec<RawTag>, Vec<ParseError>) {
    let lines: Vec<&str> = contents.lines().collect();
    let mut tags = Vec::new();
    let mut errors = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let (col, _tag_prefix) = match find_tag_in_line(lines[i]) {
            Some(r) => r,
            None => {
                i += 1;
                continue;
            }
        };

        let start_line = i + 1; // 1-based

        // Determine the comment prefix used on the opening line.
        let before_tag = &lines[i][..col];
        let comment_prefix = detect_comment_prefix(before_tag);

        // Content from `<wk` (or `<watcher-knight`) onward on this line.
        let after_tag_start = &lines[i][col..];

        // Step 2: Find the corresponding `/>`.
        if let Some(close_pos) = after_tag_start.find("/>") {
            // Single-line tag.
            let content = &after_tag_start[..close_pos];
            tags.push(RawTag {
                content: content.to_string(),
                line: start_line,
            });
            i += 1;
            continue;
        }

        // Multi-line: collect continuation lines until `/>`.
        let mut collected = after_tag_start.to_string();
        i += 1;
        let mut found_close = false;

        while i < lines.len() {
            let stripped = match strip_continuation(lines[i], comment_prefix) {
                Some(s) => s,
                None => break, // Comment block ended without `/>`.
            };

            if let Some(close_pos) = stripped.find("/>") {
                let before = stripped[..close_pos].trim_end();
                if !before.is_empty() {
                    collected.push('\n');
                    collected.push_str(before);
                }
                found_close = true;
                i += 1;
                break;
            }

            collected.push('\n');
            collected.push_str(stripped.trim());
            i += 1;
        }

        if !found_close {
            errors.push(ParseError {
                file: file.to_string(),
                line: start_line,
                message: format!(
                    "unclosed watcher tag: `{}` opened but no matching `/>` was found",
                    _tag_prefix,
                ),
            });
        } else {
            tags.push(RawTag {
                content: collected,
                line: start_line,
            });
        }
    }

    (tags, errors)
}

// ── Phase 2: nom Parsers ───────────────────────────────────────────────────────

/// Match `<wk` or `<watcher-knight`.
fn nom_tag_prefix(input: &str) -> IResult<&str, &str> {
    nom::branch::alt((tag("<watcher-knight"), tag("<wk")))(input)
}

/// Match `:` with optional surrounding whitespace.
fn nom_colon(input: &str) -> IResult<&str, char> {
    let (input, _) = space0(input)?;
    let (input, c) = char(':')(input)?;
    let (input, _) = space0(input)?;
    Ok((input, c))
}

/// Match a watcher name (alphanumeric, hyphens, underscores).
fn nom_name(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '-' || c == '_')(input)
}

/// Match a single file entry inside `[...]` (everything up to `,` or `]`).
fn nom_file_entry(input: &str) -> IResult<&str, &str> {
    let (input, _) = space0(input)?;
    let (input, entry) = take_while1(|c: char| c != ',' && c != ']')(input)?;
    Ok((input, entry.trim()))
}

/// Match an inline file list: `[file1, file2, ...]`.
fn nom_file_list(input: &str) -> IResult<&str, Vec<&str>> {
    let (input, _) = space0(input)?;
    let (input, _) = char('[')(input)?;
    let (input, _) = space0(input)?;
    let (input, files) = separated_list0(tuple((space0, char(','), space0)), nom_file_entry)(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char(']')(input)?;
    Ok((input, files))
}

/// Match a key="value" pair.
fn nom_key_value(input: &str) -> IResult<&str, (&str, &str)> {
    let (input, _) = space0(input)?;
    let (input, key) = take_while1(|c: char| c.is_alphanumeric() || c == '_')(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char('"')(input)?;
    let (input, value) = take_while(|c: char| c != '"')(input)?;
    let (input, _) = char('"')(input)?;
    Ok((input, (key, value)))
}

/// Match `options={key="value", ...}`.
fn nom_options(input: &str) -> IResult<&str, Vec<(&str, &str)>> {
    let (input, _) = tag("options")(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = space0(input)?;
    let (input, pairs) =
        separated_list0(tuple((space0, char(','), space0)), nom_key_value)(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char('}')(input)?;
    Ok((input, pairs))
}

/// Match `files = { file1, file2, ... }`.
fn nom_files_directive(input: &str) -> IResult<&str, Vec<&str>> {
    let (input, _) = tag("files")(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char('=')(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char('{')(input)?;
    let (input, _) = space0(input)?;
    let (input, files) = separated_list0(
        tuple((space0, char(','), space0)),
        take_while1(|c: char| c != ',' && c != '}'),
    )(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = char('}')(input)?;
    let files: Vec<&str> = files.iter().map(|s| s.trim()).collect();
    Ok((input, files))
}

// ── Phase 2: Tag Parsing ───────────────────────────────────────────────────────

/// Parse a raw tag content string into a `Marker`, or return a `ParseError`.
fn parse_raw_tag(
    content: &str,
    file: &str,
    line: usize,
    marker_parent: &Path,
    repo_root: &Path,
) -> Result<Marker, ParseError> {
    let err = |msg: String| ParseError {
        file: file.to_string(),
        line,
        message: msg,
    };

    // Split into first line and the rest.
    let (first_line, rest) = match content.find('\n') {
        Some(pos) => (&content[..pos], &content[pos + 1..]),
        None => (content, ""),
    };

    // Parse tag prefix.
    let remaining = match nom_tag_prefix(first_line) {
        Ok((r, _)) => r,
        Err(_) => {
            return Err(err(
                "expected `<wk` or `<watcher-knight` tag prefix".to_string(),
            ))
        }
    };

    // Parse colon.
    let remaining = match nom_colon(remaining) {
        Ok((r, _)) => r,
        Err(_) => {
            return Err(err(
                "expected `:` after tag prefix (e.g., `<wk: my-watcher ...`)".to_string(),
            ))
        }
    };

    // Parse name.
    let (remaining, name) = match nom_name(remaining) {
        Ok((r, n)) => (r, n.to_string()),
        Err(_) => {
            return Err(err(
                "expected watcher name after `<wk:` (names may contain alphanumeric characters, \
                 hyphens, and underscores)"
                    .to_string(),
            ))
        }
    };

    // Parse optional inline file list.
    let remaining_trimmed = remaining.trim_start();
    let (remaining, mut raw_files) = if remaining_trimmed.starts_with('[') {
        match nom_file_list(remaining) {
            Ok((r, files)) => (r, files),
            Err(_) => {
                return Err(err(
                    "unclosed `[` in file list: expected matching `]`".to_string(),
                ))
            }
        }
    } else {
        (remaining, Vec::new())
    };

    // Collect all remaining text (rest of first line + continuation lines).
    let mut instruction_parts: Vec<String> = Vec::new();
    let mut options: HashMap<String, String> = HashMap::new();

    // Remainder of the first line after structured parts.
    let first_remainder = remaining.trim();
    if !first_remainder.is_empty() {
        instruction_parts.push(first_remainder.to_string());
    }

    // Process body lines.
    for (offset, body_line) in rest.lines().enumerate() {
        let trimmed = body_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try options={...}
        if trimmed.starts_with("options") {
            match nom_options(trimmed) {
                Ok((_, pairs)) => {
                    for (k, v) in pairs {
                        options.insert(k.to_string(), v.to_string());
                    }
                    continue;
                }
                Err(_) => {
                    return Err(ParseError {
                        file: file.to_string(),
                        line: line + 1 + offset,
                        message: "malformed options: expected `options={key=\"value\", ...}`"
                            .to_string(),
                    });
                }
            }
        }

        // Try files = {...}
        if trimmed.starts_with("files") {
            if let Ok((_, directive_files)) = nom_files_directive(trimmed) {
                raw_files.extend(directive_files);
                continue;
            }
        }

        instruction_parts.push(trimmed.to_string());
    }

    let instruction = instruction_parts.join("\n");
    if instruction.is_empty() {
        return Err(err(format!(
            "watcher `{name}` has no instruction text"
        )));
    }

    // Resolve file paths.
    let files = resolve_raw_files(&raw_files, marker_parent, repo_root);

    Ok(Marker {
        name,
        rel_path: file.to_string(),
        line,
        instruction,
        files,
        options,
    })
}

// ── File Resolution ────────────────────────────────────────────────────────────

/// Normalize a path by resolving `.` and `..` components without touching the
/// filesystem.
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

/// Resolve raw file entries relative to the marker's parent directory, expanding
/// glob patterns against the repo root.
fn resolve_raw_files(raw: &[&str], marker_parent: &Path, repo_root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for &entry in raw {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let joined = marker_parent.join(entry);
        let normalized = normalize_path(&joined);
        let pattern_str = normalized.to_string_lossy().to_string();

        let abs_pattern = repo_root.join(&pattern_str);
        let abs_str = abs_pattern.to_string_lossy().to_string();
        match glob::glob(&abs_str) {
            Ok(paths) => {
                let mut matched = false;
                for abs_path in paths.flatten() {
                    if let Ok(rel) = abs_path.strip_prefix(repo_root) {
                        files.push(rel.to_string_lossy().to_string());
                        matched = true;
                    }
                }
                if !matched {
                    files.push(pattern_str);
                }
            }
            Err(_) => {
                files.push(pattern_str);
            }
        }
    }
    files
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Parse all watcher-knight markers from a file's contents.
///
/// Returns `(markers, errors)` — valid markers are returned even when some tags
/// fail to parse.
pub fn parse_markers(
    contents: &str,
    rel_path: &str,
    repo_root: &Path,
) -> (Vec<Marker>, Vec<ParseError>) {
    let (raw_tags, mut errors) = extract_raw_tags(contents, rel_path);
    let mut markers = Vec::new();

    let marker_parent = Path::new(rel_path).parent().unwrap_or(Path::new(""));

    for raw in raw_tags {
        match parse_raw_tag(&raw.content, rel_path, raw.line, marker_parent, repo_root) {
            Ok(marker) => markers.push(marker),
            Err(e) => errors.push(e),
        }
    }

    (markers, errors)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Helper: parse markers from a string using dummy paths.
    fn parse(contents: &str) -> (Vec<Marker>, Vec<ParseError>) {
        parse_markers(contents, "test.ts", Path::new("/repo"))
    }

    // ── Successful parsing ─────────────────────────────────────────────────

    #[test]
    fn single_line_basic() {
        let (markers, errors) = parse("// <wk: my-watcher Check something. />");
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "my-watcher");
        assert_eq!(markers[0].instruction, "Check something.");
        assert!(markers[0].files.is_empty());
        assert!(markers[0].options.is_empty());
        assert_eq!(markers[0].line, 1);
    }

    #[test]
    fn single_line_with_files() {
        let (markers, errors) =
            parse("// <wk: api-check [./a.ts, ./b.py] Ensure alignment. />");
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "api-check");
        assert_eq!(markers[0].instruction, "Ensure alignment.");
        assert_eq!(markers[0].files, vec!["a.ts", "b.py"]);
    }

    #[test]
    fn multi_line_basic() {
        let input = "\
// <wk: error-handling
// All API calls must handle errors. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "error-handling");
        assert_eq!(markers[0].instruction, "All API calls must handle errors.");
        assert_eq!(markers[0].line, 1);
    }

    #[test]
    fn multi_line_with_files_and_instruction() {
        let input = "\
// <wk: api-align [./frontend.ts, ./backend.py]
// Ensure the backend and frontend API definitions align />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "api-align");
        assert_eq!(
            markers[0].instruction,
            "Ensure the backend and frontend API definitions align"
        );
        assert_eq!(markers[0].files, vec!["frontend.ts", "backend.py"]);
    }

    #[test]
    fn multi_line_with_options() {
        let input = "\
// <wk: port-check [.]
// options={model=\"haiku\"}
// Only one service on port 5000. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "port-check");
        assert_eq!(markers[0].instruction, "Only one service on port 5000.");
        assert_eq!(markers[0].options.get("model").unwrap(), "haiku");
    }

    #[test]
    fn multi_line_with_files_directive() {
        let input = "\
// <wk: schema-sync
// files = { ./migrations/*.sql, ./models.rs }
// Ensure migrations match models. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "schema-sync");
        assert_eq!(markers[0].instruction, "Ensure migrations match models.");
        // File paths are resolved via glob; since these don't exist on disk,
        // they are kept as normalized paths.
        assert_eq!(markers[0].files.len(), 2);
    }

    #[test]
    fn multi_line_with_options_and_files_directive() {
        let input = "\
// <wk: full-check
// files = { ./a.ts, ./b.py }
// options={model=\"opus\", verbose=\"true\"}
// Check everything. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "full-check");
        assert_eq!(markers[0].instruction, "Check everything.");
        assert_eq!(markers[0].options.get("model").unwrap(), "opus");
        assert_eq!(markers[0].options.get("verbose").unwrap(), "true");
        assert_eq!(markers[0].files.len(), 2);
    }

    #[test]
    fn watcher_knight_long_prefix() {
        let input = "# <watcher-knight: long-name Check it. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "long-name");
        assert_eq!(markers[0].instruction, "Check it.");
    }

    #[test]
    fn hash_comment_style() {
        let input = "\
# <wk: py-check [./app.py]
# Validate the app. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "py-check");
    }

    #[test]
    fn double_dash_comment_style() {
        let input = "\
-- <wk: sql-check
-- Validate the query. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "sql-check");
    }

    #[test]
    fn percent_comment_style() {
        let input = "% <wk: tex-check Validate formatting. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "tex-check");
    }

    #[test]
    fn semicolon_comment_style() {
        let input = "\
; <wk: lisp-check
; Validate parens. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "lisp-check");
    }

    #[test]
    fn multiple_markers_in_one_file() {
        let input = "\
// <wk: first Check one. />
some code here
// <wk: second Check two. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].name, "first");
        assert_eq!(markers[0].line, 1);
        assert_eq!(markers[1].name, "second");
        assert_eq!(markers[1].line, 3);
    }

    #[test]
    fn name_with_underscores() {
        let (markers, errors) = parse("// <wk: my_watcher_v2 Check it. />");
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers[0].name, "my_watcher_v2");
    }

    #[test]
    fn no_space_after_colon() {
        let (markers, errors) = parse("// <wk:compact-name Check it. />");
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers[0].name, "compact-name");
        assert_eq!(markers[0].instruction, "Check it.");
    }

    #[test]
    fn indented_marker() {
        let input = "    // <wk: indented Check it. />";
        let (markers, errors) = parse(input);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "indented");
    }

    #[test]
    fn no_markers_in_file() {
        let (markers, errors) = parse("just some code\nno markers here");
        assert!(errors.is_empty());
        assert!(markers.is_empty());
    }

    #[test]
    fn wk_in_non_tag_context_ignored() {
        // `<wking>` should not be matched as a tag.
        let (markers, errors) = parse("let x = \"<wking>\";\n");
        assert!(errors.is_empty());
        assert!(markers.is_empty());
    }

    // ── Error cases ────────────────────────────────────────────────────────

    #[test]
    fn error_unclosed_tag() {
        let input = "// <wk: oops No closing tag";
        let (markers, errors) = parse(input);
        assert!(markers.is_empty());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 1);
        assert!(
            errors[0].message.contains("unclosed watcher tag"),
            "error message was: {}",
            errors[0].message,
        );
    }

    #[test]
    fn error_unclosed_tag_multiline() {
        let input = "\
// <wk: oops
// This never closes
// Still going";
        let (markers, errors) = parse(input);
        assert!(markers.is_empty());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 1);
        assert!(errors[0].message.contains("unclosed watcher tag"));
    }

    #[test]
    fn error_missing_colon() {
        let input = "// <wk no-colon-here />";
        let (markers, errors) = parse(input);
        assert!(markers.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("expected `:`"),
            "error message was: {}",
            errors[0].message,
        );
    }

    #[test]
    fn error_missing_name() {
        let input = "// <wk: />";
        let (markers, errors) = parse(input);
        assert!(markers.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("expected watcher name"),
            "error message was: {}",
            errors[0].message,
        );
    }

    #[test]
    fn error_unclosed_file_list() {
        let input = "// <wk: broken [./a.ts, ./b.py Check it. />";
        let (markers, errors) = parse(input);
        assert!(markers.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("unclosed `[`"),
            "error message was: {}",
            errors[0].message,
        );
    }

    #[test]
    fn error_empty_instruction() {
        let input = "// <wk: no-instruction />";
        let (markers, errors) = parse(input);
        assert!(markers.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("no instruction text"),
            "error message was: {}",
            errors[0].message,
        );
    }

    #[test]
    fn error_malformed_options() {
        let input = "\
// <wk: bad-opts [.]
// options={broken}
// Check it. />";
        let (markers, errors) = parse(input);
        assert!(markers.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("malformed options"),
            "error message was: {}",
            errors[0].message,
        );
    }

    #[test]
    fn error_malformed_options_line_number() {
        let input = "\
// <wk: bad-opts [.]
// options={not valid}
// Check it. />";
        let (_, errors) = parse(input);
        assert_eq!(errors.len(), 1);
        // The options line is line 2 (1-based), which is line + 1 + offset.
        // start_line = 1, offset = 0 (first body line), so error line = 1 + 1 + 0 = 2.
        assert_eq!(errors[0].line, 2);
    }

    #[test]
    fn mixed_valid_and_invalid() {
        // The unclosed tag on line 2 does NOT consume line 4 because the non-
        // comment line 3 breaks the continuation.
        let input = "\
// <wk: good Check something. />
// <wk: bad-no-close No closing tag
not a comment
// <wk: also-good Also check this. />";
        let (markers, errors) = parse(input);
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].name, "good");
        assert_eq!(markers[1].name, "also-good");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unclosed watcher tag"));
    }

    #[test]
    fn error_display_format() {
        let err = ParseError {
            file: "src/app.ts".to_string(),
            line: 42,
            message: "unclosed watcher tag".to_string(),
        };
        assert_eq!(err.to_string(), "src/app.ts:42: unclosed watcher tag");
    }

    // ── Extraction helpers ─────────────────────────────────────────────────

    #[test]
    fn find_tag_rejects_partial_match() {
        assert!(find_tag_in_line("something <wking>").is_none());
        assert!(find_tag_in_line("<wkfoo").is_none());
    }

    #[test]
    fn find_tag_accepts_valid() {
        let (pos, prefix) = find_tag_in_line("// <wk: name").unwrap();
        assert_eq!(pos, 3);
        assert_eq!(prefix, "<wk");

        let (pos, prefix) = find_tag_in_line("# <watcher-knight: name").unwrap();
        assert_eq!(pos, 2);
        assert_eq!(prefix, "<watcher-knight");
    }

    #[test]
    fn detect_comment_prefix_works() {
        assert_eq!(detect_comment_prefix("  // "), Some("//"));
        assert_eq!(detect_comment_prefix("# "), Some("#"));
        assert_eq!(detect_comment_prefix("  -- "), Some("--"));
        assert_eq!(detect_comment_prefix("let x = "), None);
    }

    // ── nom parser unit tests ──────────────────────────────────────────────

    #[test]
    fn nom_parse_name_valid() {
        assert_eq!(nom_name("my-watcher rest"), Ok((" rest", "my-watcher")));
        assert_eq!(nom_name("a_b_c"), Ok(("", "a_b_c")));
        assert_eq!(nom_name("v2-check!"), Ok(("!", "v2-check")));
    }

    #[test]
    fn nom_parse_file_list_valid() {
        let (rest, files) = nom_file_list(" [./a.ts, ./b.py]").unwrap();
        assert_eq!(rest, "");
        assert_eq!(files, vec!["./a.ts", "./b.py"]);
    }

    #[test]
    fn nom_parse_file_list_single() {
        let (_, files) = nom_file_list("[.]").unwrap();
        assert_eq!(files, vec!["."]);
    }

    #[test]
    fn nom_parse_options_valid() {
        let (_, pairs) = nom_options("options={model=\"haiku\"}").unwrap();
        assert_eq!(pairs, vec![("model", "haiku")]);
    }

    #[test]
    fn nom_parse_options_multiple() {
        let (_, pairs) = nom_options("options={model=\"opus\", verbose=\"true\"}").unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("model", "opus"));
        assert_eq!(pairs[1], ("verbose", "true"));
    }

    #[test]
    fn nom_parse_files_directive_valid() {
        let (_, files) = nom_files_directive("files = { ./a.ts, ./b.py }").unwrap();
        assert_eq!(files, vec!["./a.ts", "./b.py"]);
    }

    #[test]
    fn normalize_path_resolves_dots() {
        assert_eq!(
            normalize_path(Path::new("example/./frontend.ts")),
            PathBuf::from("example/frontend.ts"),
        );
        assert_eq!(
            normalize_path(Path::new("example/../src/main.rs")),
            PathBuf::from("src/main.rs"),
        );
    }
}
