//! Reachability probing (TCP connect + optional HTTP request).

use std::net::IpAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Clone, Debug)]
pub struct ProbeResult {
    pub reachable: bool,
    pub reason: String,
    pub http_status: Option<String>,
    pub response_time_ms: u128,
}

/// Probe a single target for reachability.
/// If tcp_only is true, only do TCP connect.
/// Otherwise, do TCP connect + HTTP request.
pub async fn probe_target(
    target: &crate::input::Target,
    cfg: &crate::config::ReachCfg,
    http_client: &reqwest::Client,
) -> ProbeResult {
    let start = std::time::Instant::now();
    let addr = match target.ip {
        IpAddr::V4(v4) => std::net::SocketAddr::new(v4.into(), target.port),
        IpAddr::V6(v6) => std::net::SocketAddr::new(v6.into(), target.port),
    };

    // Try TCP connect with retries on timeout
    let mut last_err = None;
    for attempt in 0..=cfg.retries {
        match timeout(cfg.timeout, TcpStream::connect(addr)).await {
            Ok(Ok(_stream)) => {
                // TCP connected successfully
                if cfg.tcp_only {
                    return ProbeResult {
                        reachable: true,
                        reason: "tcp connected".to_string(),
                        http_status: None,
                        response_time_ms: start.elapsed().as_millis(),
                    };
                }

                // Try HTTP request
                let scheme = &target.scheme;
                let host_header = match target.ip {
                    IpAddr::V6(v6) => format!("[{}]:{}", v6, target.port),
                    IpAddr::V4(v4) => format!("{}:{}", v4, target.port),
                };
                let url = format!("{}://{}/", scheme, host_header);

                match timeout(cfg.timeout, http_client.get(&url).send()).await {
                    Ok(Ok(resp)) => {
                        let status = resp.status().as_u16();
                        return ProbeResult {
                            reachable: true,
                            reason: format!("http {}", status),
                            http_status: Some(status.to_string()),
                            response_time_ms: start.elapsed().as_millis(),
                        };
                    }
                    Ok(Err(e)) => {
                        return ProbeResult {
                            reachable: false,
                            reason: format!("http error: {}", e),
                            http_status: None,
                            response_time_ms: start.elapsed().as_millis(),
                        };
                    }
                    Err(_) => {
                        return ProbeResult {
                            reachable: false,
                            reason: "http timeout".to_string(),
                            http_status: None,
                            response_time_ms: start.elapsed().as_millis(),
                        };
                    }
                }
            }
            Ok(Err(e)) => {
                // Connection refused or other error
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("refused") || err_str.contains("reset") {
                    return ProbeResult {
                        reachable: false,
                        reason: "connection refused".to_string(),
                        http_status: None,
                        response_time_ms: start.elapsed().as_millis(),
                    };
                }
                last_err = Some(e.to_string());
                // Retry on timeout-like errors
                if attempt < cfg.retries {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
            Err(_) => {
                // Timeout
                last_err = Some("tcp timeout".to_string());
                if attempt < cfg.retries {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }

    ProbeResult {
        reachable: false,
        reason: last_err.unwrap_or_else(|| "unknown error".to_string()),
        http_status: None,
        response_time_ms: start.elapsed().as_millis(),
    }
}