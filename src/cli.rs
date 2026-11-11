//! Command-line interface (minimal args, smart defaults).

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "hostreach",
    about = "🔍 Fast, reliable CLI tool to check reachable hosts from a URL list. No TLS verification, async, configurable concurrency.",
    version
)]
pub struct Args {
    /// Input .txt file with URLs (http://ip:port, https://domain, etc.) one per line
    pub input: PathBuf,

    /// Output file for reachable hosts (default: reachable_<unix_timestamp>.txt)
    pub output: Option<PathBuf>,

    /// Verbose/debug logs to stderr
    #[arg(short, long)]
    pub verbose: bool,

    /// Max concurrent probes (default: auto-tuned from system ulimit/nofile)
    #[arg(long)]
    pub concurrency: Option<usize>,

    /// Connect/read timeout in milliseconds (default: 1000)
    #[arg(long)]
    pub timeout_ms: Option<u64>,

    /// Retries per host on timeout only (default: 2). Refused (RST) is never retried.
    #[arg(long)]
    pub retries: Option<u32>,

    /// Only check TCP connectivity (skip HTTP request)
    #[arg(long)]
    pub tcp_only: bool,

    /// Follow redirects (default: false)
    #[arg(long)]
    pub follow_redirects: bool,
}