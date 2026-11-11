//! Output formatting and file writing.

use anyhow::Result;
use std::path::Path;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::input::Target;

/// Append content to file with a mutex lock for thread safety.
pub async fn append_file(path: &Path, lock: &Mutex<()>, content: &str) -> Result<()> {
    let _guard = lock.lock().await;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(content.as_bytes()).await?;
    Ok(())
}

/// Write content to file (overwrites), with a mutex lock for thread safety.
pub async fn write_file(path: &Path, lock: &Mutex<()>, content: &str) -> Result<()> {
    let _guard = lock.lock().await;
    tokio::fs::write(path, content).await?;
    Ok(())
}

/// Format a reachable target for output (just the original URL).
pub fn format_reachable(target: &Target) -> String {
    format!("{}\n", target.original)
}

/// Format an unreachable target for verbose output (unused but kept for API completeness).
#[allow(dead_code)]
pub fn format_unreachable(target: &Target, reason: &str) -> String {
    format!("❌ {} -> {} (unreachable: {})\n", target.original, target.ip, reason)
}