# hostreach

[![Crates.io](https://img.shields.io/crates/v/hostreach.svg)](https://crates.io/crates/hostreach)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://github.com/mnk-works/hostreach/actions/workflows/rust.yml/badge.svg)](https://github.com/mnk-works/hostreach/actions/workflows/rust.yml)

🔍 **Fast, reliable CLI tool to check reachable hosts from a URL list.**

- Reads URLs from a file (`http://ip:port`, `https://domain`, `https://domain:port/path`, etc.)
- Checks reachability via TCP connect + optional HTTP request
- **No TLS certificate verification** — works with self-signed/expired certs
- Async, configurable concurrency (auto-tuned from `ulimit -n`)
- Retries on timeout, skips refused connections immediately
- Writes reachable hosts to output file (one per line, original URL format)
- Append-safe output — safe to interrupt and resume

## Installation

### From crates.io (recommended)
```bash
cargo install hostreach
```

### From source
```bash
git clone https://github.com/mnk-works/hostreach
cd hostreach
cargo install --path .
```

### Pre-built binaries
Download from [GitHub Releases](https://github.com/mnk-works/hostreach/releases).

## Usage

```bash
# Basic usage
hostreach input.txt

# Custom output file
hostreach input.txt -o reachable.txt

# Verbose output (shows unreachable hosts)
hostreach input.txt -v

# TCP connect only (skip HTTP request, faster)
hostreach input.txt --tcp-only

# Follow HTTP redirects
hostreach input.txt --follow-redirects

# Custom concurrency, timeout, retries
hostreach input.txt --concurrency 1000 --timeout-ms 2000 --retries 3

# Help
hostreach --help
```

### Input Format

One URL per line in `input.txt`:
```
http://192.0.2.1:8080
https://example.com
https://example.com:8443/path
http://[2001:db8::1]:80
# comments are ignored
```

Supported schemes: `http`, `https` (default ports: 80, 443)

### Output Format

`reachable_<timestamp>.txt` contains:
```
# Header with scan metadata
http://192.0.2.1:8080
https://example.com
https://example.com:8443/path
# Summary with per-host results
```

Only reachable hosts are written (one per line, original URL format preserved).

## Examples

### Check a list of web servers
```bash
# input.txt
https://google.com
https://github.com
http://192.0.2.1:8080
https://expired.badssl.com

# Run
hostreach input.txt -v
```

### Quick TCP-only port check
```bash
hostreach input.txt --tcp-only --timeout-ms 500
```

### High-throughput scan
```bash
hostreach input.txt --concurrency 5000 --retries 1
```

## How It Works

1. **Parse & Resolve**: Reads input file, parses URLs, resolves hostnames to IPs (IPv4 preferred)
2. **Deduplicate**: Removes duplicate IP:port combinations
3. **Probe**: For each target:
   - TCP connect with configurable timeout
   - Retries on timeout (configurable, default 2)
   - Skips retry on connection refused (RST) — definitive and fast
   - If not `--tcp-only`: sends HTTP GET request, accepts any status code
4. **Output**: Writes reachable hosts to output file immediately (append-safe)

## TLS Handling

**Certificate validation is disabled** (`danger_accept_invalid_certs(true)`). This means:
- Self-signed certificates ✅
- Expired certificates ✅
- Wrong hostname in cert ✅
- Any TLS error is treated as reachable (TCP connected)

This is intentional for reachability checking — we only care if the host responds.

## Concurrency Tuning

Concurrency is auto-tuned from `ulimit -n` (open file descriptors):
- Reads soft/hard limits
- Attempts to raise soft limit to hard limit
- Reserves headroom for stdin/stdout/stderr, DNS, HTTP pool
- Clamped to 512–4000 by default
- Override with `--concurrency`

## License

MIT License — see [LICENSE](LICENSE) for details.