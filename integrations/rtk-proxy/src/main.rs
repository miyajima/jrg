use clap::Parser;
use jrg_rtk_proxy::{build_router, SelectionConfig};
use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(about = "Opt-in Anthropic RTK Proxy with origin-bound jrg evidence selection")]
struct Args {
    #[arg(long)]
    selector: PathBuf,
    #[arg(long)]
    anthropic_upstream: String,
    #[arg(long, default_value = "127.0.0.1:8081")]
    bind: SocketAddr,
    #[arg(long)]
    search_tool: Vec<String>,
    #[arg(long)]
    page_tool: Vec<String>,
    #[arg(long, default_value_t = 1000)]
    max_tokens: usize,
    #[arg(long, default_value_t = 10)]
    selection_timeout_secs: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !args.bind.ip().is_loopback() {
        return Err("--bind must be a loopback IP address".into());
    }
    if !args.selector.is_absolute() || !args.selector.is_file() {
        return Err("--selector must identify an existing absolute executable path".into());
    }
    if args.max_tokens == 0 || args.selection_timeout_secs == 0 {
        return Err("token budget and selection timeout must be greater than zero".into());
    }
    if !args.anthropic_upstream.starts_with("http://")
        && !args.anthropic_upstream.starts_with("https://")
    {
        return Err("--anthropic-upstream must be an explicit HTTP(S) base URL".into());
    }
    let searches = if args.search_tool.is_empty() {
        vec!["search".into()]
    } else {
        args.search_tool
    };
    let pages = if args.page_tool.is_empty() {
        vec!["fetch".into()]
    } else {
        args.page_tool
    };
    if searches
        .iter()
        .chain(pages.iter())
        .any(|name| name.trim().is_empty())
        || searches.iter().any(|name| pages.contains(name))
    {
        return Err("tool names must be nonempty and search/page names must not overlap".into());
    }
    let selection = SelectionConfig {
        selector: args.selector.canonicalize()?,
        search_tools: searches,
        page_tools: pages,
        max_tokens: args.max_tokens,
        timeout: Duration::from_secs(args.selection_timeout_secs),
    };
    let router = build_router(selection, args.anthropic_upstream);
    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    eprintln!(
        "jrg-rtk-proxy listening on {}; Anthropic Messages only",
        listener.local_addr()?
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
