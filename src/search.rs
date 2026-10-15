use crate::{
    cli::{Args, Encoding},
    normalize::{self, Options},
};
use encoding_rs::{EUC_JP, SHIFT_JIS};
use regex::{Regex, RegexBuilder};
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, BufRead, BufReader},
    ops::Range,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
};

pub struct Matcher {
    regex: Regex,
    options: Options,
}

impl Matcher {
    pub fn new(patterns: &[String], args: &Args) -> Result<Self, regex::Error> {
        let options = Options {
            nfkc: !args.regex && !args.no_normalize,
            kana: !args.regex && !args.no_normalize && !args.no_kana,
        };
        let patterns: Vec<_> = patterns
            .iter()
            .map(|p| normalize::normalize(p, options))
            .collect();
        let insensitive = args.ignore_case
            || (args.smart_case && !patterns.iter().any(|p| p.chars().any(char::is_uppercase)));
        let joined = patterns
            .iter()
            .map(|p| {
                if args.regex {
                    format!("(?:{p})")
                } else {
                    regex::escape(p)
                }
            })
            .collect::<Vec<_>>()
            .join("|");
        let regex = RegexBuilder::new(&joined)
            .case_insensitive(insensitive)
            .build()?;
        Ok(Self { regex, options })
    }

    fn matches(&self, line: &str) -> bool {
        self.regex
            .is_match(&normalize::normalize(line, self.options))
    }

    fn ranges(&self, line: &str) -> Vec<Range<usize>> {
        let text = normalize::normalize(line, self.options);
        normalize::original_ranges(
            line,
            self.regex.find_iter(&text).map(|m| m.range()),
            self.options,
        )
    }
}

pub struct Line {
    pub path: Arc<String>,
    pub number: u64,
    pub text: String,
    pub ranges: Vec<Range<usize>>,
    pub selected: bool,
}

pub enum Event {
    Line(Line),
    File(Arc<String>),
    Count(Arc<String>, u64),
    Found,
    Error(String),
}

pub struct Search<'a> {
    pub args: &'a Args,
    pub matcher: Matcher,
    pub sender: SyncSender<Event>,
    pub stop: AtomicBool,
}

impl Search<'_> {
    pub fn emit(&self, event: Event) {
        if self.sender.send(event).is_err() {
            self.stop.store(true, Ordering::Relaxed);
        }
    }

    pub fn error(&self, message: impl Into<String>) {
        self.emit(Event::Error(message.into()));
    }

    pub fn file(&self, path: &Path) {
        if self.stop.load(Ordering::Relaxed) {
            return;
        }
        match File::open(path) {
            Ok(file) => self.reader(BufReader::new(file), path.to_string_lossy().into_owned()),
            Err(err) => self.error(format!("{}: {err}", path.display())),
        }
    }

    pub fn reader(&self, reader: impl BufRead, label: String) {
        if let Err(err) = self.scan(reader, Arc::new(label.clone())) {
            self.error(format!("{label}: {err}"));
        }
    }

    fn scan(&self, mut reader: impl BufRead, path: Arc<String>) -> io::Result<()> {
        // Like grep, classify from an initial buffer; -a permits embedded NULs.
        if !self.args.text && reader.fill_buf()?.contains(&0) {
            return Ok(());
        }
        let before = self.args.context.max(self.args.before_context);
        let after = self.args.context.max(self.args.after_context);
        let mut history: VecDeque<(u64, String)> = VecDeque::new();
        let mut last_emitted = 0;
        let mut after_until = 0;
        let mut number = 0_u64;
        let mut count = 0_u64;
        let mut buffer = Vec::new();
        loop {
            if self.stop.load(Ordering::Relaxed) {
                break;
            }
            buffer.clear();
            if reader.read_until(b'\n', &mut buffer)? == 0 {
                break;
            }
            number += 1;
            if buffer.last() == Some(&b'\n') {
                buffer.pop();
                if buffer.last() == Some(&b'\r') {
                    buffer.pop();
                }
            }
            let bytes = if number == 1 && matches!(self.args.encoding, Encoding::Utf8) {
                buffer.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&buffer)
            } else {
                &buffer
            };
            let text = decode(bytes, self.args.encoding).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{number} 行目が指定した文字コードでは読めません（--encoding を確認）"),
                )
            })?;
            let limit_reached = self.args.max_count.is_some_and(|max| count >= max);
            let selected =
                !limit_reached && (self.matcher.matches(&text) != self.args.invert_match);
            if selected {
                count += 1;
                if count == 1 {
                    self.emit(Event::Found);
                }
                if self.args.quiet {
                    self.stop.store(true, Ordering::Relaxed);
                    break;
                }
                if self.args.files_with_matches {
                    self.emit(Event::File(path.clone()));
                    break;
                }
                if !self.args.count {
                    for (n, context) in history.drain(..) {
                        if n > last_emitted {
                            self.emit(Event::Line(Line {
                                path: path.clone(),
                                number: n,
                                text: context,
                                ranges: vec![],
                                selected: false,
                            }));
                        }
                    }
                    let ranges = if self.args.invert_match {
                        vec![]
                    } else {
                        self.matcher.ranges(&text)
                    };
                    self.emit(Event::Line(Line {
                        path: path.clone(),
                        number,
                        text: text.clone(),
                        ranges,
                        selected: true,
                    }));
                    last_emitted = number;
                    after_until = number.saturating_add(after as u64);
                }
            } else if !self.args.count
                && !self.args.files_with_matches
                && !self.args.quiet
                && number <= after_until
            {
                self.emit(Event::Line(Line {
                    path: path.clone(),
                    number,
                    text: text.clone(),
                    ranges: vec![],
                    selected: false,
                }));
                last_emitted = number;
            }
            if before > 0 && !self.args.count && !self.args.files_with_matches && !self.args.quiet {
                if history.len() == before {
                    history.pop_front();
                }
                history.push_back((number, text));
            }
            if self.args.max_count.is_some_and(|max| count >= max)
                && (self.args.count || number >= after_until)
            {
                break;
            }
        }
        if self.args.count && count > 0 {
            self.emit(Event::Count(path, count));
        }
        Ok(())
    }
}

fn decode(bytes: &[u8], encoding: Encoding) -> Result<String, ()> {
    match encoding {
        Encoding::Utf8 => std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| ()),
        Encoding::ShiftJis | Encoding::EucJp => {
            let codec = match encoding {
                Encoding::ShiftJis => SHIFT_JIS,
                _ => EUC_JP,
            };
            codec
                .decode_without_bom_handling_and_without_replacement(bytes)
                .map(|text| text.into_owned())
                .ok_or(())
        }
    }
}
