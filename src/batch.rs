//! Typed, read-only operations. Never a shell or a command execution interface.
use crate::{
    blocks::{Capture, Evidence},
    cli::Args,
};
use clap::CommandFactory;
use serde_json::Value;
use std::{
    error::Error,
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

enum Operation {
    Search(Box<Args>),
    Read(String, usize, usize),
    Error(String),
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

pub fn run(args: &Args) -> Result<u8, Box<dyn Error>> {
    let value = if let Some(path) = args.batch.as_ref() {
        let reader: Box<dyn Read> = if path == Path::new("-") {
            Box::new(io::stdin())
        } else {
            Box::new(File::open(path)?)
        };
        let mut input = Vec::new();
        reader.take(1024 * 1024 + 1).read_to_end(&mut input)?;
        if input.len() > 1024 * 1024 {
            return Err(invalid("batch exceeds 1 MiB").into());
        }
        let value: Value = serde_json::from_slice(&input)?;
        Some(value)
    } else {
        None
    };
    execute(args, value, &mut io::stdout().lock())
}

pub fn execute(
    args: &Args,
    value: Option<Value>,
    writer: &mut dyn Write,
) -> Result<u8, Box<dyn Error>> {
    let requirements = crate::requirements::Requirements::parse(
        value.as_ref().and_then(|v| v.get("requirements")),
        args.max_tokens as usize,
    )?;
    if args
        .rank_query
        .as_ref()
        .is_some_and(|q| crate::rank::tokens(q).is_empty())
    {
        return Err(invalid("--rank-query needs at least one searchable term").into());
    }
    if args.intent == crate::cli::Intent::Review
        && (!args.smart
            || !args.block_context
            || !matches!(args.encoding, crate::cli::Encoding::Utf8)
            || args.max_tokens < 128)
    {
        return Err(
            invalid("review needs smart UTF-8 scopes, no explicit context, budget >=128").into(),
        );
    }
    let mut session = args
        .session
        .as_ref()
        .map(|p| crate::session::Session::open(p))
        .transpose()?;
    let mut prepared = Vec::new();
    if let Some(value) = value {
        if value.as_object().is_none_or(|o| {
            !o.contains_key("operations")
                || o.keys().any(|k| k != "operations" && k != "requirements")
        }) {
            return Err(invalid("expected operations and optional requirements").into());
        }
        let operations = value["operations"]
            .as_array()
            .ok_or_else(|| invalid("operations must be an array"))?;
        if operations.is_empty() || operations.len() > 64 {
            return Err(invalid("batch needs 1..64 operations").into());
        }
        // Validate the complete plan before performing any operation.
        for op in operations {
            let object = op
                .as_object()
                .filter(|o| o.len() == 1)
                .ok_or_else(|| invalid("each operation needs exactly search or read"))?;
            if let Some(search) = object.get("search") {
                let array = search
                    .as_array()
                    .ok_or_else(|| invalid("search must be an argv array"))?;
                let argv: Vec<&str> = array
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| invalid("search arguments must be strings"))
                    })
                    .collect::<Result<_, _>>()?;
                let matches = match Args::command().try_get_matches_from(
                    std::iter::once("jrg")
                        .chain(std::iter::once("--smart"))
                        .chain(argv),
                ) {
                    Ok(matches) => matches,
                    Err(err) => {
                        prepared.push(Operation::Error(err.to_string()));
                        continue;
                    }
                };
                let scope_explicit = matches.value_source("explain_scope")
                    == Some(clap::parser::ValueSource::CommandLine);
                let mut inner = Args::prepared(&matches);
                if !scope_explicit {
                    inner.explain_scope = args.explain_scope;
                }
                if inner.mcp
                    || inner.rank_query.is_some()
                    || inner.discovery_limit.is_some()
                    || inner.root.is_some()
                    || inner.session.is_some()
                    || inner.resume
                    || inner.repeat
                    || inner.batch.is_some()
                    || inner.json
                    || inner.count
                    || inner.files_with_matches
                    || inner.quiet
                    || inner.only_matching
                    || inner.ai
                    || inner.no_line_number
                    || inner.column
                {
                    prepared.push(Operation::Error("batch search needs numbered content search, no nested batches or machine modes; --rank-query belongs on the outer command".into()));
                    continue;
                }
                if inner.intent == crate::cli::Intent::Review
                    && (!inner.block_context
                        || !matches!(inner.encoding, crate::cli::Encoding::Utf8)
                        || args.max_tokens < 128)
                {
                    prepared.push(Operation::Error(
                        "review needs UTF-8 scopes, no explicit context, budget >=128".into(),
                    ));
                    continue;
                }
                let (patterns, paths) = inner.inputs(true);
                if paths.iter().any(|p| p == Path::new("-")) {
                    prepared.push(Operation::Error("batch search cannot read stdin".into()));
                    continue;
                }
                inner.pattern = None;
                inner.patterns = patterns;
                inner.paths = paths;
                prepared.push(Operation::Search(Box::new(inner)));
            } else if let Some(read) = object.get("read") {
                let obj = read
                    .as_object()
                    .filter(|o| o.len() == 3)
                    .ok_or_else(|| invalid("read needs path, start, end"))?;
                let path = obj
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("read path must be a string"))?;
                let start = obj
                    .get("start")
                    .and_then(Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| invalid("invalid read start"))?;
                let end = obj
                    .get("end")
                    .and_then(Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| invalid("invalid read end"))?;
                if start == 0 || end < start {
                    return Err(invalid("read uses a 1-based inclusive range").into());
                }
                prepared.push(Operation::Read(path.into(), start, end));
            } else {
                return Err(
                    invalid("unknown batch operation; only search and read are allowed").into(),
                );
            }
        }
    } else if !args.resume {
        let mut inner = args.clone();
        if args.intent == crate::cli::Intent::Review {
            let (patterns, paths) = args.inputs(true);
            if paths.iter().any(|p| p == Path::new("-")) {
                return Err(invalid("review cannot read stdin").into());
            }
            inner.pattern = None;
            inner.patterns = patterns;
            inner.paths = paths;
        }
        prepared.push(Operation::Search(Box::new(inner)));
    }
    let mut evidence = Evidence::default();
    evidence.requirements = requirements;
    evidence.rank_query = args.rank_query.clone();
    evidence.discovery_limit = args.discovery_limit.unwrap_or(12) as usize;
    if args.resume {
        evidence.resume(session.as_mut().unwrap())?;
    }
    let mut search_code = 0;
    let mut failed = false;
    for (i, op) in prepared.into_iter().enumerate() {
        let result: Result<(), Box<dyn Error>> = match op {
            Operation::Error(message) => Err(invalid(&message).into()),
            Operation::Read(path, start, end) => {
                evidence.read(&path, start, end).map_err(Into::into)
            }
            Operation::Search(inner) => {
                let mut capture = if inner.intent == crate::cli::Intent::Review {
                    Capture::preview_for(
                        args.rank_query
                            .as_deref()
                            .unwrap_or(&inner.patterns.join(" ")),
                    )
                } else {
                    Capture::default()
                };
                match crate::run(&inner, &mut io::sink(), false, Some(&mut capture)) {
                    Ok(code) => {
                        if code == 1 {
                            if args.batch.is_none() {
                                search_code = 1;
                            }
                            evidence
                                .messages
                                .push(format!("operation={} no matches", i + 1));
                        }
                        if code == 2 {
                            failed = true;
                            evidence.messages.push(format!(
                                "operation={} search error; results may be partial",
                                i + 1
                            ));
                        }
                        if inner.intent == crate::cli::Intent::Review {
                            evidence
                                .review(&inner, capture, session.as_ref(), args.repeat)
                                .map_err(Into::into)
                        } else {
                            evidence.search(&inner, capture).map_err(Into::into)
                        }
                    }
                    Err(err) => Err(err),
                }
            }
        };
        if let Err(err) = result {
            failed = true;
            evidence
                .messages
                .push(format!("operation={} error: {err}", i + 1));
        }
    }
    failed |= evidence
        .messages
        .iter()
        .any(|m| m.contains("error:") || m.contains("search error"));
    let tokenizer = tiktoken_rs::o200k_base_singleton();
    let evidence_semantic_found = evidence.semantic_found();
    let payload = evidence.render_tracked(
        tokenizer,
        args.max_tokens as usize,
        session.as_mut(),
        args.repeat,
    );
    match writer.write_all(payload.as_bytes()) {
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => return Ok(0),
        other => other?,
    }
    writer.flush()?;
    if let Some(session) = session {
        session.save()?;
    }
    Ok(if failed {
        2
    } else if evidence_semantic_found {
        0
    } else {
        search_code
    })
}
