//! API key management CLI commands.
use anyhow::Result;
use colored::Colorize;

pub fn create_key(label: &str) -> Result<()> {
    let store = tet_core::auth::KeyStore::new();
    let raw = store.create_key(label.to_string());
    println!(
        "{} API key created: {}",
        "✅".green(),
        raw.bright_green().bold()
    );
    println!("   Label: {}", label.cyan());
    println!(
        "{} Store this key securely. It won't be shown again.",
        "⚠".yellow()
    );
    Ok(())
}

pub fn list_keys() -> Result<()> {
    let store = tet_core::auth::KeyStore::new();
    let keys = store.list();
    if keys.is_empty() {
        println!("{} No active API keys.", "ℹ".blue());
        return Ok(());
    }
    println!("{} Active API keys:", "🔑".bold());
    for (prefix, count, label) in &keys {
        println!("  {}  {} calls  {}", prefix.cyan(), count, label.dimmed());
    }
    Ok(())
}

pub fn revoke_key(prefix: &str) -> Result<()> {
    let store = tet_core::auth::KeyStore::new();
    if store.revoke(prefix) {
        println!("{} Revoked key {}", "✅".green(), prefix.cyan());
    } else {
        anyhow::bail!("Key not found: {}", prefix);
    }
    Ok(())
}
