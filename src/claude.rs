use std::io::Write;
use std::process;
use std::sync::mpsc;
use std::thread;

use crate::marker::Marker;
use crate::prompt;

struct WatcherResult {
    name: String,
    location: String,
    is_valid: bool,
    reason: Option<String>,
}

pub fn run_watchers(markers: &[Marker], diff: &str) {
    let n = markers.len();
    eprintln!("running {n} watchers\n");

    let (tx, rx) = mpsc::channel();

    for marker in markers {
        let tx = tx.clone();
        let name = marker.name.clone();
        let location = format!("{}:{}", marker.rel_path, marker.line);
        let prompt_text = prompt::build_watcher_prompt(marker, diff);

        thread::spawn(move || {
            let result = run_single_watcher(&name, &location, &prompt_text);
            tx.send(result).ok();
        });
    }
    drop(tx);

    let mut results: Vec<WatcherResult> = Vec::new();
    let mut completed = 0;

    for result in rx {
        completed += 1;
        let status = if result.is_valid {
            "\x1b[32mOK\x1b[0m"
        } else {
            "\x1b[31mFAILED\x1b[0m"
        };
        eprintln!("[{completed}/{n}] {}... {status}", result.name);
        results.push(result);
    }

    // Final output to stdout
    println!();
    println!("---- RESULTS ----");
    println!();
    for r in &results {
        let status = if r.is_valid {
            "\x1b[32mOK\x1b[0m"
        } else {
            "\x1b[31mFAILED\x1b[0m"
        };
        println!("watcher {}... {status}", r.name);
    }

    let failures: Vec<_> = results.iter().filter(|r| !r.is_valid).collect();
    if !failures.is_empty() {
        println!();
        println!("\x1b[31m---- FAILURES ----");
        for f in &failures {
            println!();
            println!("---- {} ({}) ----", f.name, f.location);
            println!();
            println!("{}", f.reason.as_deref().unwrap_or("unknown reason"));
        }
        print!("\x1b[0m");
    }

    let passed = results.iter().filter(|r| r.is_valid).count();
    let failed = failures.len();
    println!();
    if failed == 0 {
        println!("watcher-knight result: \x1b[32mOK\x1b[0m. {passed} passed; 0 failed");
    } else {
        println!("watcher-knight result: \x1b[31mFAILED\x1b[0m. {passed} passed; {failed} failed");
        process::exit(1);
    }
}

fn run_single_watcher(name: &str, location: &str, prompt: &str) -> WatcherResult {
    let mut child = process::Command::new("claude")
        .args([
            "-p",
            "--model",
            "haiku",
            "--permission-mode",
            "dontAsk",
            "--allowedTools",
            "Read,Grep,Glob",
        ])
        .env_remove("CLAUDECODE")
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::null())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to launch claude for watcher {name}: {e}");
            process::exit(1);
        });

    child
        .stdin
        .take()
        .unwrap()
        .write_all(prompt.as_bytes())
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to write prompt for watcher {name}: {e}");
            process::exit(1);
        });

    let output = child.wait_with_output().unwrap_or_else(|e| {
        eprintln!("Error: failed to wait on claude for watcher {name}: {e}");
        process::exit(1);
    });

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if !output.status.success() {
        return WatcherResult {
            name: name.to_string(),
            location: location.to_string(),
            is_valid: false,
            reason: Some(format!("claude process exited with {}", output.status)),
        };
    }

    parse_response(name, location, &text)
}

fn parse_response(name: &str, location: &str, text: &str) -> WatcherResult {
    // Try to find a JSON object in the response
    let json_str = extract_json(text).unwrap_or(text);

    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(val) => {
            let is_valid = val
                .get("is_valid")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let reason = if !is_valid {
                val.get("reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| Some("marked invalid with no reason".to_string()))
            } else {
                None
            };
            WatcherResult {
                name: name.to_string(),
                location: location.to_string(),
                is_valid,
                reason,
            }
        }
        Err(_) => WatcherResult {
            name: name.to_string(),
            location: location.to_string(),
            is_valid: false,
            reason: Some(format!("malformed response: {text}")),
        },
    }
}

/// Find the first `{ ... }` substring that looks like JSON.
fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0;
    for (i, ch) in text[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}
