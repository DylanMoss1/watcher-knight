use std::fmt::Write as _;

use crate::marker::Marker;

pub fn build_watcher_prompt(marker: &Marker, diff: Option<&str>) -> String {
    let mut out = String::new();

    let diff_instruction = if diff.is_some() {
        "Use the diff to understand what changed, then ALWAYS use Read/Grep/Glob to \
         verify the invariant against the actual codebase."
    } else {
        "ALWAYS use Read/Grep/Glob to verify the invariant against the actual codebase."
    };

    writeln!(
        out,
        "You are validating a code invariant.\n\
         \n\
         Invariant name: {}\n\
         File: {} (line {})\n\
         Instruction: {}\n\
         \n\
         Check whether the current state of the code satisfies this invariant.\n\
         {diff_instruction} You must confirm that any \
         files or code referenced by the invariant actually exist. If a file referenced \
         by the invariant does not exist, the invariant is violated.\n\
         \n\
         Respond with ONLY a JSON object, no other text:\n\
         - {{\"is_valid\": true}} if the invariant holds\n\
         - {{\"is_valid\": false, \"reason\": \"...\"}} if it is violated\n\
         \n\
         IMPORTANT: Your reason will be shown directly to the end user. \
         Write it as a clear, actionable description of the problem. \
         Do NOT reference diffs, HEAD, commits, or the validation process itself. \
         Just describe what is wrong with the code.",
        marker.name, marker.rel_path, marker.line, marker.instruction,
    )
    .unwrap();

    if let Some(diff) = diff {
        writeln!(out).unwrap();
        writeln!(out, "## Diff (HEAD → working tree)").unwrap();
        writeln!(out, "```diff").unwrap();
        write!(out, "{diff}").unwrap();
        if !diff.ends_with('\n') {
            writeln!(out).unwrap();
        }
        writeln!(out, "```").unwrap();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_marker(name: &str, instruction: &str) -> Marker {
        Marker {
            name: name.to_string(),
            rel_path: "src/app.ts".to_string(),
            line: 42,
            instruction: instruction.to_string(),
            files: vec![],
            options: HashMap::new(),
        }
    }

    #[test]
    fn prompt_contains_marker_fields() {
        let m = make_marker("my-check", "Ensure alignment");
        let out = build_watcher_prompt(&m, None);
        assert!(out.contains("my-check"));
        assert!(out.contains("src/app.ts"));
        assert!(out.contains("42"));
        assert!(out.contains("Ensure alignment"));
    }

    #[test]
    fn prompt_no_diff_has_no_diff_section() {
        let m = make_marker("test", "Check it");
        let out = build_watcher_prompt(&m, None);
        assert!(!out.contains("## Diff"));
        assert!(!out.contains("```diff"));
    }

    #[test]
    fn prompt_no_diff_instruction_text() {
        let m = make_marker("test", "Check it");
        let out = build_watcher_prompt(&m, None);
        assert!(out.contains("ALWAYS use Read/Grep/Glob"));
        assert!(!out.contains("Use the diff to understand"));
    }

    #[test]
    fn prompt_with_diff_has_diff_section() {
        let m = make_marker("test", "Check it");
        let out = build_watcher_prompt(&m, Some("+ added line\n"));
        assert!(out.contains("## Diff"));
        assert!(out.contains("```diff"));
        assert!(out.contains("+ added line"));
    }

    #[test]
    fn prompt_with_diff_instruction_text() {
        let m = make_marker("test", "Check it");
        let out = build_watcher_prompt(&m, Some("diff"));
        assert!(out.contains("Use the diff to understand what changed"));
        assert!(out.contains("ALWAYS use Read/Grep/Glob"));
    }

    #[test]
    fn prompt_contains_json_format() {
        let m = make_marker("test", "Check it");
        let out = build_watcher_prompt(&m, None);
        assert!(out.contains("\"is_valid\""));
        assert!(out.contains("JSON"));
    }

    #[test]
    fn prompt_diff_without_trailing_newline_adds_one() {
        let m = make_marker("test", "Check it");
        let out = build_watcher_prompt(&m, Some("no trailing newline"));
        // Should have newline before closing fence
        assert!(out.contains("no trailing newline\n```"));
    }

    #[test]
    fn prompt_diff_with_trailing_newline_no_double() {
        let m = make_marker("test", "Check it");
        let out = build_watcher_prompt(&m, Some("has newline\n"));
        assert!(out.contains("has newline\n```"));
        assert!(!out.contains("has newline\n\n```"));
    }

    #[test]
    fn prompt_diff_empty_string() {
        let m = make_marker("test", "Check it");
        let out = build_watcher_prompt(&m, Some(""));
        assert!(out.contains("## Diff"));
        assert!(out.contains("```diff"));
    }
}
