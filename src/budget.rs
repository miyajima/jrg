//! Exact o200k counting with bounded reuse inside one rendering operation.
pub struct Counter<'a> {
    tokenizer: &'a tiktoken_rs::CoreBPE,
    memo: std::cell::RefCell<Memo>,
}
#[derive(Default)]
struct Memo {
    counts: std::collections::HashMap<String, usize>,
    bytes: usize,
}
impl<'a> Counter<'a> {
    pub fn new(tokenizer: &'a tiktoken_rs::CoreBPE) -> Self {
        Self {
            tokenizer,
            memo: Default::default(),
        }
    }
    pub fn count(&self, text: &str) -> usize {
        // o200k's regex pieces cannot cross a newline followed by these
        // non-whitespace markers. In particular '/' is excluded: the
        // punctuation alternative can consume slashes AFTER a newline.
        // Count complete regex regions, never add arbitrary BPE fragments.
        let bytes = text.as_bytes();
        let mut start = 0;
        let mut total = 0;
        for (n, byte) in bytes.iter().enumerate() {
            if *byte == b'\n'
                && bytes
                    .get(n + 1)
                    .is_some_and(|b| matches!(b, b'#' | b'@' | b'0'..=b'9'))
            {
                total += self.piece(&text[start..n + 1]);
                start = n + 1;
            }
        }
        total + self.piece(&text[start..])
    }
    fn piece(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        if let Some(count) = self.memo.borrow().counts.get(text) {
            return *count;
        }
        let count = self.tokenizer.encode_ordinary(text).len();
        let mut memo = self.memo.borrow_mut();
        if memo.counts.len() < 8192 && memo.bytes.saturating_add(text.len()) <= 2 * 1024 * 1024 {
            memo.bytes += text.len();
            memo.counts.insert(text.to_owned(), count);
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_utf8_and_full_cache_keep_the_whole_encoding_count() {
        let tokenizer = tiktoken_rs::o200k_base_singleton();
        let counter = Counter::new(tokenizer);
        let alphabet = [
            '\n', '\r', ' ', '\t', '/', '#', '@', '0', '9', 'a', 'Z', '\'', '!', '日', '😀',
            '\u{301}', '\u{2028}', '\u{a0}',
        ];
        let mut seed = 4711u64;
        for n in 0..2000 {
            let mut text = String::new();
            for _ in 0..n % 180 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                text.push(alphabet[(seed >> 32) as usize % alphabet.len()]);
            }
            assert_eq!(
                counter.count(&text),
                tokenizer.encode_ordinary(&text).len(),
                "{text:?}"
            );
        }
        for n in 0..9000 {
            counter.count(&format!("{n}: unique_{n}\n"));
        }
        let large = "0".repeat(2 * 1024 * 1024 + 1);
        assert_eq!(
            counter.count(&large),
            tokenizer.encode_ordinary(&large).len()
        );
        assert!(counter.memo.borrow().bytes <= 2 * 1024 * 1024);
        assert!(counter.memo.borrow().counts.len() <= 8192);
        assert_eq!(
            counter.count("1:changed\n# status\n"),
            tokenizer.encode_ordinary("1:changed\n# status\n").len()
        );
    }
    #[test]
    fn counts_match_whole_encoding_at_bpe_boundaries_and_after_appends() {
        let tokenizer = tiktoken_rs::o200k_base_singleton();
        let counter = Counter::new(tokenizer);
        let fragments = [
            "",
            " ",
            "\t",
            "\r\n",
            "\n",
            "\n/",
            "//\n/",
            "# note\n",
            "@ 条件.rs:1-2\n",
            "123:日本語😀e\u{301}ガ\n",
            "!\r\n#x",
            "don't\n123",
            "\u{2028}",
            "<|endoftext|>",
        ];
        for a in fragments {
            for b in fragments {
                for c in fragments {
                    let text = format!("{a}{b}{c}");
                    assert_eq!(
                        counter.count(&text),
                        tokenizer.encode_ordinary(&text).len(),
                        "{text:?}"
                    );
                }
            }
        }
        let mut source = String::new();
        for n in 0..500 {
            source.push_str(&format!(
                "{n}: if status != \"completed\" || 条件_{n} {{ //末尾 /\n"
            ));
            let candidate = format!("{source}# scope=heuristic pending={}\n", 500 - n);
            assert_eq!(
                counter.count(&candidate),
                tokenizer.encode_ordinary(&candidate).len()
            );
        }
    }
}
