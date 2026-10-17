mod ai;
mod batch;
mod blocks;
mod budget;
mod cli;
mod embedding;
mod mcp;
mod normalize;
mod output;
mod rank;
mod requirements;
mod search;
mod session;
mod smart;
mod syntax;

use clap::CommandFactory;
use cli::{Args, Color, Engine};
use ignore::{WalkBuilder, WalkState, overrides::OverrideBuilder};
use output::Output;
use search::{Event, Matcher, Search};
use std::{
    io::{self, BufWriter, IsTerminal, Write},
    path::Path,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::sync_channel,
    },
};

fn main() -> ExitCode {
    let matches = Args::command().get_matches();
    let args = Args::prepared(&matches);
    let stdout = io::stdout();
    let result = if args.mcp {
        mcp::serve(&args)
    } else if args.batch.is_some()
        || args.session.is_some()
        || args.rank_query.is_some()
        || args.intent == cli::Intent::Review
    {
        batch::run(&args)
    } else {
        run(
            &args,
            &mut BufWriter::new(stdout.lock()),
            stdout.is_terminal(),
            None,
        )
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("jrg: {err}");
            ExitCode::from(2)
        }
    }
}

fn run(
    args: &Args,
    writer: &mut dyn Write,
    terminal: bool,
    mut capture: Option<&mut blocks::Capture>,
) -> Result<u8, Box<dyn std::error::Error>> {
    let (patterns, paths) = args.inputs(io::stdin().is_terminal());
    let matcher = Matcher::new(&patterns, args)?;
    let eligible = smart::rg_eligible(args, &paths);
    if args.engine == Engine::Rg && !eligible {
        return Err("rg cannot preserve this request; normalization, stdin, encoding or output mode needs native".into());
    }
    let requested_rg =
        args.engine == Engine::Rg || (args.engine == Engine::Auto && args.smart && eligible);
    let external = if requested_rg {
        match smart::RgProcess::spawn(args, &patterns, &paths) {
            Ok(process) => Some(process),
            Err(err) if err.kind() == io::ErrorKind::NotFound && args.engine == Engine::Auto => {
                None
            }
            Err(err) => return Err(err.into()),
        }
    } else {
        None
    };
    if args.explain_plan {
        eprintln!(
            "jrg: engine={} reason={} context_before={} context_after={} output={}",
            if external.is_some() { "rg" } else { "native" },
            if external.is_some() {
                "raw-search"
            } else if requested_rg {
                "rg-unavailable"
            } else if eligible {
                "native-requested"
            } else {
                "native-semantics"
            },
            args.context.max(args.before_context),
            args.context.max(args.after_context),
            if args.smart {
                "adaptive"
            } else if args.ai {
                "ai"
            } else {
                "standard"
            }
        );
    }
    // The vocabulary is embedded in the executable; there is no runtime download.
    let tokenizer = if (args.ai || args.smart) && capture.is_none() {
        Some(tiktoken_rs::o200k_base()?)
    } else {
        None
    };
    let mut ai_output = args
        .ai
        .then(|| ai::AiOutput::new(args.snippet_chars as usize));
    let mut smart_output = (args.smart && !args.block_context && capture.is_none())
        .then(|| smart::SmartOutput::new(args, &paths));
    let mut scoped = blocks::Capture::default();
    let mut overrides = OverrideBuilder::new(std::env::current_dir()?);
    for glob in &args.globs {
        overrides.add(glob)?;
    }
    let overrides = overrides.build()?;
    let use_stdin = paths.iter().any(|p| p == Path::new("-"));
    let paths: Vec<_> = paths.into_iter().filter(|p| p != Path::new("-")).collect();
    let mut walker = paths.first().map(WalkBuilder::new);
    if let Some(walker) = &mut walker {
        for path in &paths[1..] {
            walker.add(path);
        }
        let threads = if args.threads == 0 {
            std::thread::available_parallelism()
                .map_or(1, usize::from)
                .min(12)
        } else {
            args.threads
        };
        walker
            .standard_filters(!args.no_ignore)
            .hidden(!args.hidden)
            .require_git(false)
            .overrides(overrides)
            .threads(threads);
    }
    let (sender, receiver) = sync_channel(128);
    let search = Search {
        args,
        matcher,
        sender,
        stop: AtomicBool::new(false),
    };
    let color = match args.color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => terminal && std::env::var_os("NO_COLOR").is_none(),
    };
    let mut output = Output {
        args,
        writer,
        color,
    };
    let mut found = false;
    let mut failed = false;
    let mut write_error = None;
    std::thread::scope(|scope| {
        scope.spawn(move || {
            if let Some(process) = external {
                process.produce(&search);
                return;
            }
            if use_stdin {
                search.reader(io::stdin().lock(), "<stdin>".into());
            }
            if let Some(walker) = walker {
                walker.build_parallel().run(|| {
                    Box::new(|entry| {
                        if search.stop.load(Ordering::Relaxed) {
                            return WalkState::Quit;
                        }
                        match entry {
                            Err(err) => search.error(err.to_string()),
                            Ok(entry) => {
                                if let Some(err) = entry.error() {
                                    search.error(err.to_string());
                                }
                                if entry.file_type().is_some_and(|kind| kind.is_file()) {
                                    search.file(entry.path());
                                }
                            }
                        }
                        WalkState::Continue
                    })
                });
            }
            // Dropping the last sender closes the output loop.
        });
        for event in receiver {
            match event {
                Event::Found => found = true,
                Event::Error(err) => {
                    eprintln!("jrg: {err}");
                    failed = true;
                }
                Event::Line(line) if capture.is_some() => {
                    capture.as_deref_mut().unwrap().push(line)
                }
                Event::Line(line) if args.block_context => scoped.push(line),
                Event::Line(line) if smart_output.is_some() => {
                    if let Err(err) = smart_output.as_mut().unwrap().push(args, line) {
                        write_error = Some(err);
                        break;
                    }
                }
                Event::Line(line) if ai_output.is_some() => {
                    ai_output.as_mut().unwrap().push(line);
                }
                _ => {
                    if let Err(err) = output.event(event) {
                        write_error = Some(err);
                        break;
                    }
                }
            }
        }
    });
    if let Some(err) = write_error {
        if err.kind() == io::ErrorKind::BrokenPipe {
            return Ok(0);
        }
        return Err(err.into());
    }
    if let Some(ai_output) = ai_output.filter(|_| capture.is_none()) {
        let payload = ai_output.finish(tokenizer.as_ref().unwrap(), args.max_tokens as usize);
        if let Err(err) = output.writer.write_all(payload.as_bytes()) {
            if err.kind() == io::ErrorKind::BrokenPipe {
                return Ok(0);
            }
            return Err(err.into());
        }
    }
    if args.block_context && capture.is_none() && found {
        let mut evidence = blocks::Evidence::default();
        if failed {
            evidence
                .messages
                .push("search error; results may be partial".into());
        }
        evidence.search(args, scoped)?;
        let payload = evidence.render(tokenizer.as_ref().unwrap(), args.max_tokens as usize);
        if let Err(err) = output.writer.write_all(payload.as_bytes()) {
            if err.kind() == io::ErrorKind::BrokenPipe {
                return Ok(0);
            }
            return Err(err.into());
        }
    }
    if let Some(smart_output) = smart_output {
        let payload = smart_output.finish(tokenizer.as_ref().unwrap(), args.max_tokens as usize);
        if let Err(err) = output.writer.write_all(payload.as_bytes()) {
            if err.kind() == io::ErrorKind::BrokenPipe {
                return Ok(0);
            }
            return Err(err.into());
        }
    }
    if let Err(err) = output.writer.flush() {
        if err.kind() == io::ErrorKind::BrokenPipe {
            return Ok(0);
        }
        return Err(err.into());
    }
    // grep/rg convention: quiet succeeds as soon as any match is found.
    Ok(if args.quiet && found {
        0
    } else if failed {
        2
    } else if found {
        0
    } else {
        1
    })
}
