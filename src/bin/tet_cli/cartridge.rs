//! Cartridge registry commands — publish and search via GitHub Releases.

use anyhow::Result;
use colored::Colorize;
use sha2::{Digest, Sha256};
use std::process::Command;

/// Publish a cartridge WASM to GitHub Releases.
pub async fn publish_cmd(path: &str, tag: &str) -> Result<()> {
    let wasm = std::fs::read(path).map_err(|e| anyhow::anyhow!("Cannot read {}: {}", path, e))?;

    // Compute content ID
    let mut hasher = Sha256::new();
    hasher.update(&wasm);
    let cid = hex::encode(hasher.finalize());
    println!("{} Cartridge CID: {}", "📦".bold(), cid.blue());
    println!(
        "{} Publishing {} ({} bytes)...",
        "⬆".bold(),
        tag.cyan(),
        wasm.len()
    );

    // Use gh CLI to create a GitHub Release with the WASM as an asset
    let status = Command::new("gh")
        .args(["release", "create", tag, path])
        .args(["--title", tag])
        .args(["--notes", &format!("Cartridge: {}\nCID: {}", tag, cid)])
        .args(["--prerelease"])
        .status()?;

    if status.success() {
        println!(
            "{} Published {} (CID: {})",
            "✅".green(),
            tag.cyan(),
            cid.dimmed()
        );
    } else {
        anyhow::bail!("Publish failed. Is gh CLI authenticated? Run: gh auth login");
    }
    Ok(())
}

/// Search GitHub Releases for cartridge artifacts.
pub async fn search_cmd(query: &str) -> Result<()> {
    println!("{} Searching for '{}'...", "🔍".bold(), query.cyan());

    let output = Command::new("gh")
        .args(["release", "list", "--limit", "50"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Search failed. Is gh CLI authenticated?");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let matches: Vec<&str> = stdout
        .lines()
        .filter(|line| line.to_lowercase().contains(&query.to_lowercase()))
        .collect();

    if matches.is_empty() {
        println!("{} No cartridges found for '{}'", "ℹ".blue(), query);
        return Ok(());
    }

    println!("{} Found {} cartridge(s):", "📋".bold(), matches.len());
    for line in matches {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(tag) = parts.first() {
            let title = parts.get(2).copied().unwrap_or("");
            println!("  {} — {}", tag.cyan(), title);
        }
    }
    Ok(())
}

/// Validate a cartridge WASM binary.
pub async fn validate_cmd(path: &str) -> Result<()> {
    let wasm = std::fs::read(path).map_err(|e| anyhow::anyhow!("Cannot read {}: {}", path, e))?;

    let mut hasher = Sha256::new();
    hasher.update(&wasm);
    let cid = hex::encode(hasher.finalize());
    println!("{} Cartridge CID: {}", "📦".bold(), cid.blue());

    if wasm.len() < 8 || &wasm[0..4] != b"\0asm" {
        anyhow::bail!("Not a valid WebAssembly binary (missing magic bytes)");
    }
    let version = u32::from_le_bytes([wasm[4], wasm[5], wasm[6], wasm[7]]);
    if version != 1 {
        anyhow::bail!("Unsupported Wasm version: {} (expected 1)", version);
    }
    println!("  ✅ Valid Wasm v1 binary");
    println!(
        "  📏 Size: {} bytes ({:.2} MB)",
        wasm.len(),
        wasm.len() as f64 / 1_048_576.0
    );
    println!("{} Cartridge validation passed.", "✅".green());
    Ok(())
}
