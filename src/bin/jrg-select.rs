use clap::{Parser, ValueEnum};
use jrg::{
    adapters,
    selection::{QueryRuleMode, SelectionRequest, select_with_query_rules, select_with_rule_ids},
};
use std::{
    io::{self, Read},
    path::PathBuf,
};

#[derive(Clone, Copy, ValueEnum)]
enum Adapter {
    Documents,
    SearchResults,
    MarkdownPage,
}

#[derive(Clone, Copy, ValueEnum)]
enum QueryRules {
    Verified,
    Builtin,
    Off,
}

#[derive(Parser)]
#[command(about = "Select original evidence from supplied JSON; never searches or fetches URLs")]
struct Args {
    #[arg(long, value_enum, default_value = "documents")]
    adapter: Adapter,
    /// Verified approvals by default; builtin is an experimental all-rules comparison.
    #[arg(long, value_enum, default_value = "verified")]
    query_rules: QueryRules,
    /// Offline comparison subset, only with --query-rules builtin.
    #[arg(long, value_delimiter = ',', requires = "query_rules")]
    rule_ids: Option<Vec<String>>,
    /// Audit normalized request identity without selecting evidence.
    #[arg(long)]
    rule_request_fingerprint: bool,
    /// Print the bundled approval policy identities without reading input.
    #[arg(long)]
    query_policy_info: bool,
    /// JSON request file, or - for stdin.
    #[arg(default_value = "-")]
    input: PathBuf,
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    if args.rule_ids.is_some() && !matches!(args.query_rules, QueryRules::Builtin) {
        return Err("--rule-ids requires --query-rules builtin".into());
    }
    if args.query_policy_info {
        println!("{}", jrg::verified_query_rules::policy_info());
        return Ok(());
    }
    let reader: Box<dyn Read> = if args.input.as_os_str() == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(std::fs::File::open(args.input).map_err(|e| e.to_string())?)
    };
    let mut bytes = Vec::new();
    reader
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("input exceeds 4 MiB".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let request = match args.adapter {
        Adapter::Documents => {
            serde_json::from_value::<SelectionRequest>(value).map_err(|e| e.to_string())?
        }
        adapter => {
            let query = value
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or("query must be a string")?
                .to_owned();
            let max_tokens = value
                .get("max_tokens")
                .and_then(|v| v.as_u64())
                .and_then(|v| usize::try_from(v).ok())
                .ok_or("max_tokens must be an integer")?;
            let query_expansions = value
                .get("query_expansions")
                .map(|v| serde_json::from_value::<Vec<String>>(v.clone()))
                .transpose()
                .map_err(|e| format!("query_expansions must be an array of strings: {e}"))?
                .unwrap_or_default();
            let input = value.get("input").ok_or("input is required")?;
            let documents = match adapter {
                Adapter::SearchResults => adapters::search_results(input)?,
                Adapter::MarkdownPage => adapters::markdown_page(input)?,
                Adapter::Documents => unreachable!(),
            };
            SelectionRequest {
                query,
                query_expansions,
                max_tokens,
                documents,
            }
        }
    };
    if args.rule_request_fingerprint {
        println!(
            "{}",
            serde_json::json!({"request_sha256": jrg::verified_query_rules::request_fingerprint(&request)?})
        );
        return Ok(());
    }
    let mode = match args.query_rules {
        QueryRules::Verified => QueryRuleMode::Verified,
        QueryRules::Builtin => QueryRuleMode::Builtin,
        QueryRules::Off => QueryRuleMode::Off,
    };
    let response = match args.rule_ids {
        Some(ids) => select_with_rule_ids(&request, &ids)?,
        None => select_with_query_rules(&request, mode)?,
    };
    println!(
        "{}",
        serde_json::to_string(&response).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("jrg-select: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
