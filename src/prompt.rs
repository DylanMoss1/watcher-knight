use std::fmt::Write as _;

use crate::marker::Marker;

pub fn build_watcher_prompt(marker: &Marker, diff: &str) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "You are validating a code invariant against a diff.\n\
         \n\
         Invariant name: {}\n\
         File: {} (line {})\n\
         Instruction: {}\n\
         \n\
         Check whether the current state of the code satisfies this invariant.\n\
         Use the diff to understand what changed, then ALWAYS use Read/Grep/Glob to \
         verify the invariant against the actual codebase. You must confirm that any \
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

    writeln!(out).unwrap();
    writeln!(out, "## Diff (HEAD → working tree)").unwrap();
    writeln!(out, "```diff").unwrap();
    write!(out, "{diff}").unwrap();
    if !diff.ends_with('\n') {
        writeln!(out).unwrap();
    }
    writeln!(out, "```").unwrap();
    out
}
