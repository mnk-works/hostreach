//! hostreach — fast, reliable host reachability checker.
//!
//! Reads URLs from a file (http://ip:port, https://domain, etc.), checks reachability
//! via TCP connect + optional HTTP request, and writes reachable hosts to output.
//! TLS cert validation is disabled. Errors never fail the scan.

mod cli;
mod config;
mod input;
mod output;
mod probe;

use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use cli::Args;
use config::{tune_ulimit_and_concurrency, ReachCfg};
use input::{resolve_targets, vlog};
use output::{append_file, write_file, format_reachable};
use probe::probe_target;

fn unix_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Smart defaults: ulimit-aware concurrency, 1000ms timeout, 2 retries on timeout.
    let (concurrency, _nofile, ulimit_note) = tune_ulimit_and_concurrency(args.concurrency);
    let timeout_ms = args.timeout_ms.unwrap_or(1000);
    let retries = args.retries.unwrap_or(2);
    let cfg = ReachCfg {
        concurrency,
        timeout: Duration::from_millis(timeout_ms),
        retries,
        verbose: args.verbose,
        tcp_only: args.tcp_only,
        follow_redirects: args.follow_redirects,
    };

    let out = args.output.clone().unwrap_or_else(|| PathBuf::from(format!("reachable_{}.txt", unix_ts())));
    let targets = resolve_targets(&args.input, args.verbose).await?;
    if targets.is_empty() {
        anyhow::bail!("no targets resolved from {}", args.input.display());
    }

    // Shared HTTP client (TLS cert validation disabled; errors never fail the scan).
    let mut http_builder = reqwest::Client::builder()
        .timeout(cfg.timeout)
        .danger_accept_invalid_certs(true)
        .pool_max_idle_per_host(cfg.concurrency.min(64))
        .user_agent(concat!("hostreach/", env!("CARGO_PKG_VERSION")));

    if cfg.follow_redirects {
        http_builder = http_builder.redirect(reqwest::redirect::Policy::limited(10));
    } else {
        http_builder = http_builder.redirect(reqwest::redirect::Policy::none());
    }

    let http = http_builder.build()?;

    let file_lock = Arc::new(Mutex::new(()));
    let start_wall = unix_ts();
    let start = Instant::now();

    let header = format!(
        "🔍 hostreach v{} results | start={} | targets={} | concurrency={} | timeout_ms={} | retries={} | tcp_only={} | {}\ninput={}\n{}\n",
        env!("CARGO_PKG_VERSION"),
        start_wall,
        targets.len(),
        cfg.concurrency,
        timeout_ms,
        retries,
        cfg.tcp_only,
        ulimit_note,
        args.input.display(),
        "=".repeat(72)
    );
    write_file(&out, &file_lock, &header).await?;
    eprintln!(
        "🚀 hostreach v{}: {} targets → {} (append) | c={} t={}ms r={} tcp_only={} | {}",
        env!("CARGO_PKG_VERSION"),
        targets.len(),
        out.display(),
        cfg.concurrency,
        timeout_ms,
        retries,
        cfg.tcp_only,
        ulimit_note
    );

    let pb = ProgressBar::new(targets.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("⏳ {spinner} [{bar:30.cyan/blue}] {pos}/{len} hosts | {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.enable_steady_tick(Duration::from_millis(120));

    let mut reachable_count: usize = 0;
    let mut host_results: BTreeMap<String, (bool, u128, Option<String>)> = BTreeMap::new();

    for (i, t) in targets.iter().enumerate() {
        pb.set_message(format!("probing {}:{} ({})", t.ip, t.port, t.scheme));
        vlog(cfg.verbose, format!("▶️  [{}/{}] {}:{}", i + 1, targets.len(), t.ip, t.port));

        let result = probe_target(t, &cfg, &http).await;
        let reachable = result.reachable;

        host_results.insert(
            format!("{}:{}", t.ip, t.port),
            (reachable, result.response_time_ms, result.http_status.clone()),
        );

        if reachable {
            reachable_count += 1;
            append_file(&out, &file_lock, &format_reachable(t)).await?;
            let status = result.http_status.map(|s| format!(" HTTP {}", s)).unwrap_or_default();
            pb.println(format!("✅ {}:{} ({}){} [{:.0}ms]", t.ip, t.port, t.scheme, status, result.response_time_ms as f64));
        } else {
            if cfg.verbose {
                pb.println(format!("❌ {}:{} ({}) [{:.0}ms] {}", t.ip, t.port, t.scheme, result.response_time_ms as f64, result.reason));
            }
        }
        pb.inc(1);
    }

    let dur = start.elapsed();
    let dur_s = dur.as_secs_f64().max(0.001);
    let end_wall = unix_ts();

    let mut summary = format!(
        "\n{}\n📊 SUMMARY | end={} | duration={:.1}s | targets={} | reachable={} | unreachable={} | c={} t={}ms r={} tcp_only={}\n",
        "=".repeat(72),
        end_wall,
        dur_s,
        targets.len(),
        reachable_count,
        targets.len() - reachable_count,
        cfg.concurrency,
        timeout_ms,
        retries,
        cfg.tcp_only
    );

    for (addr, (reach, ms, http_status)) in &host_results {
        let status_str = if *reach { "✅" } else { "❌" };
        let http_str = http_status.as_ref().map(|s| format!(" HTTP {}", s)).unwrap_or_default();
        summary.push_str(&format!("  {} {} ({}ms{})\n", status_str, addr, ms, http_str));
    }

    summary.push_str(&format!("📄 output: {}\n", out.display()));
    append_file(&out, &file_lock, &summary).await?;
    pb.finish_with_message("done");
    eprintln!("{}", summary);

    // Desktop notification via OSC 99 (kitty protocol)
    let msg = format!(
        "hostreach done: {} targets, {} reachable ({:.1}s)",
        targets.len(),
        reachable_count,
        dur_s
    );
    let notif = format!("\x1b]99;i=hostreach:d=1:p=body:s=system;{}\x1b\\", msg);
    use std::io::Write as _;
    let mut se = std::io::stderr();
    let _ = se.write_all(notif.as_bytes());
    let _ = se.write_all(b"\n");
    let _ = se.flush();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::input::parse_input_line;
    use super::config::tune_ulimit_and_concurrency;

    #[test]
    fn parse_http_url() {
        let t = parse_input_line("http://203.0.113.10:7777/").unwrap();
        assert_eq!(t.host, "203.0.113.10");
        assert_eq!(t.port, 7777);
        assert_eq!(t.scheme, "http");
    }

    #[test]
    fn parse_https_url() {
        let t = parse_input_line("https://example.com/path").unwrap();
        assert_eq!(t.host, "example.com");
        assert_eq!(t.port, 443);
        assert_eq!(t.scheme, "https");
    }

    #[test]
    fn parse_https_with_port() {
        let t = parse_input_line("https://example.com:8443/").unwrap();
        assert_eq!(t.host, "example.com");
        assert_eq!(t.port, 8443);
        assert_eq!(t.scheme, "https");
    }

    #[test]
    fn parse_ipv6_url() {
        let t = parse_input_line("http://[::1]:8080/").unwrap();
        // IPv6 brackets are stripped by parser
        assert_eq!(t.host, "::1");
        assert_eq!(t.port, 8080);
        assert_eq!(t.scheme, "http");
    }

    #[test]
    fn parse_skips_comments_blanks() {
        assert!(parse_input_line("").is_none());
        assert!(parse_input_line("   # hi").is_none());
        assert!(parse_input_line("ftp://example.com").is_none());
    }

    #[test]
    fn ulimit_tuning_caps_safely() {
        let (c, _, note) = tune_ulimit_and_concurrency(None);
        assert!(c >= 256);
        assert!(c <= 4000);
        assert!(note.contains("nofile"));
    }
}