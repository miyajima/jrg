//! Count stdin using the same embedded tokenizer as --ai.
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    println!(
        "{}",
        tiktoken_rs::o200k_base()?.encode_ordinary(&text).len()
    );
    Ok(())
}
