//! JSONL helpers for the frozen web-selection experiment (no network).
use std::io::{self, BufRead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tokenizer = tiktoken_rs::o200k_base()?;
    let counter = jrg::budget::Counter::new(&tokenizer);
    for line in io::stdin().lock().lines() {
        let input: serde_json::Value = serde_json::from_str(&line?)?;
        let output = match input["op"].as_str() {
            Some("count") => {
                serde_json::json!({"tokens":counter.count(input["text"].as_str().ok_or("text required")?)})
            }
            Some("prepare") => {
                let docs = match input["adapter"].as_str() {
                    Some("search-results") => jrg::adapters::search_results(&input["input"]),
                    Some("markdown-page") => jrg::adapters::markdown_page(&input["input"]),
                    _ => return Err("unsupported adapter".into()),
                }?;
                let selected = docs
                    .into_iter()
                    .map(|doc| {
                        let hash = format!(
                            "{:x}",
                            <sha2::Sha256 as sha2::Digest>::digest(
                                serde_json::to_vec(&doc).unwrap()
                            )
                        );
                        jrg::selection::SelectedDocument {
                            document: doc,
                            source_hash: hash,
                        }
                    })
                    .collect::<Vec<_>>();
                serde_json::json!({"documents":selected})
            }
            _ => return Err("unsupported operation".into()),
        };
        println!("{output}");
    }
    Ok(())
}
