//! Input parsing (URLs/hosts) and DNS resolution.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Target {
    pub original: String,
    pub host: String,
    pub ip: IpAddr,
    pub port: u16,
    pub scheme: String, // "http" or "https"
}

pub fn vlog(v: bool, msg: impl AsRef<str>) {
    if v {
        eprintln!("🛠️  {}", msg.as_ref());
    }
}

/// Parse a line into a Target. Accepts URLs like:
/// - http://ip:port
/// - https://domain
/// - https://domain:port/path
/// - http://[ipv6]:port
/// Defaults: http -> port 80, https -> port 443
pub fn parse_input_line(line: &str) -> Option<Target> {
    let s = line.trim();
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    let original = s.to_string();

    let u = url::Url::parse(s).ok()?;
    let scheme = u.scheme().to_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    // url::Url::host_str() returns IPv6 hosts with brackets, strip them
    let host = u.host_str()?.trim_start_matches('[').trim_end_matches(']').to_string();
    let port = u.port_or_known_default().unwrap_or(if scheme == "https" { 443 } else { 80 });

    Some(Target {
        original,
        host,
        ip: "0.0.0.0".parse().unwrap(), // placeholder, will be resolved
        port,
        scheme,
    })
}

/// Resolve all targets from a file, de-duplicating by IP:port combination.
/// Hostnames/domains are resolved via system DNS.
pub async fn resolve_targets(path: &Path, verbose: bool) -> Result<Vec<Target>> {
    let text = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    let mut out = Vec::new();
    let mut seen: BTreeSet<(IpAddr, u16)> = BTreeSet::new();

    for line in text.lines() {
        let Some(mut target) = parse_input_line(line) else { continue };

        let ip = if let Ok(ip) = target.host.parse::<IpAddr>() {
            ip
        } else {
            let h = target.host.trim_end_matches('.').to_string();
            vlog(verbose, format!("🌐 resolving {}", h));
            let h2 = h.clone();
            let resolved = tokio::task::spawn_blocking(move || (h2.as_str(), 0u16).to_socket_addrs()).await;
            let addrs = match resolved {
                Ok(Ok(a)) => a,
                Ok(Err(e)) => {
                    eprintln!("⚠️  skip unresolved host {} ({}). continuing.", target.host, e);
                    continue;
                }
                Err(e) => {
                    eprintln!("⚠️  skip unresolved host {} (resolver task: {}). continuing.", target.host, e);
                    continue;
                }
            };
            match addrs
                .map(|sa: SocketAddr| sa.ip())
                .find(|ip| ip.is_ipv4())
                .or_else(|| (h.as_str(), 0u16).to_socket_addrs().ok().and_then(|mut a| a.next().map(|s| s.ip())))
            {
                Some(ip) => {
                    vlog(verbose, format!("🌐 {} -> {}", h, ip));
                    ip
                }
                None => {
                    eprintln!("⚠️  skip unresolved host {} (no addresses). continuing.", target.host);
                    continue;
                }
            }
        };

        target.ip = ip;
        if seen.insert((ip, target.port)) {
            out.push(target);
        } else {
            vlog(verbose, format!("↪️  dupe {}:{} (from {})", ip, target.port, target.original));
        }
    }
    Ok(out)
}