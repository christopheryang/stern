//! Talk to the downloaded model by hand, with no Stern rules in between.
//!
//! ```text
//! cargo run --release -p stern-core --example chat_with_model
//! ```
//!
//! Reads the installed model and runtime from Stern's app-data folder (so
//! nothing is downloaded and nothing leaves the computer). Type a message
//! and press Return; an empty line or Ctrl-D quits. Set `STERN_SYSTEM` to
//! change the system prompt, or `STERN_PROMPTS` to a file with one message
//! per line to run them unattended.

#![allow(clippy::expect_used, clippy::print_stdout, clippy::unwrap_used)]

use std::io::{BufRead, Write};
use std::path::PathBuf;
use stern_core::ai::model_manager::PromptStyle;
use stern_core::ai::AiModel;

fn app_data() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Application Support/com.stern.app/models")
}

fn find_in(dir: &std::path::Path, suffix: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_in(&path, suffix) {
                return Some(found);
            }
        } else if path.to_string_lossy().ends_with(suffix) {
            return Some(path);
        }
    }
    None
}

fn main() {
    let root = app_data();
    let runtime = find_in(&root.join("runtime"), ".dylib").expect("no runtime library installed");
    let model = find_in(&root, ".onnx").expect("no model installed");
    let tokenizer = find_in(&root, "tokenizer.json").expect("no tokenizer installed");
    println!("model: {}", model.display());

    let started = std::time::Instant::now();
    let ai = AiModel::new(&runtime, &model, &tokenizer, PromptStyle::ChatMl).expect("load model");
    println!("loaded in {:?}\n", started.elapsed());

    let system = std::env::var("STERN_SYSTEM")
        .unwrap_or_else(|_| "You are a helpful assistant.".to_string());
    let mut history: Vec<(String, String)> = vec![("system".into(), system)];

    let scripted: Option<Vec<String>> = std::env::var("STERN_PROMPTS")
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.lines().map(str::to_string).collect());
    let mut lines = scripted.map(Vec::into_iter);
    let stdin = std::io::stdin();

    loop {
        let line = if let Some(it) = lines.as_mut() {
            match it.next() {
                Some(line) => line,
                None => break,
            }
        } else {
            print!("you> ");
            std::io::stdout().flush().ok();
            let mut buf = String::new();
            if stdin.lock().read_line(&mut buf).unwrap_or(0) == 0 {
                break;
            }
            buf.trim().to_string()
        };
        if line.is_empty() {
            break;
        }
        if lines.is_some() {
            println!("you> {line}");
        }
        history.push(("user".into(), line));
        let turns: Vec<(&str, &str)> = history
            .iter()
            .map(|(r, t)| (r.as_str(), t.as_str()))
            .collect();
        let started = std::time::Instant::now();
        match ai.chat_for_experiment(&turns, 200) {
            Ok(reply) => {
                println!("model> {}\n  ({:?})\n", reply.trim(), started.elapsed());
                history.push(("assistant".into(), reply));
            }
            Err(e) => {
                println!("error: {e}");
                break;
            }
        }
    }
}
