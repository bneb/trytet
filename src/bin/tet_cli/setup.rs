use std::path::PathBuf;

/// Register Trytet as an MCP server in the given config files.
/// Each entry in `configs` is (display_name, config_path).
/// `binary_path` is the absolute path written into the MCP config.
pub fn register_mcp(binary_path: &str, configs: &[(&str, PathBuf)]) -> Vec<(String, String)> {
    let trytet_entry = serde_json::json!({
        "command": binary_path,
        "args": ["mcp"]
    });

    let mut results = Vec::new();

    for (name, config_path) in configs {
        if !config_path.exists() {
            continue;
        }

        let content = match std::fs::read_to_string(config_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut config: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if config
            .get("mcpServers")
            .and_then(|s| s.get("trytet"))
            .is_some()
        {
            results.push((name.to_string(), "already_configured".into()));
            continue;
        }

        if let Some(mcp) = config.as_object_mut() {
            let servers = mcp
                .entry("mcpServers")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(obj) = servers.as_object_mut() {
                obj.insert("trytet".to_string(), trytet_entry.clone());
            }
        }

        let formatted = serde_json::to_string_pretty(&config).unwrap_or_default();
        if std::fs::write(config_path, formatted).is_ok() {
            results.push((name.to_string(), "added".into()));
        } else {
            results.push((name.to_string(), "write_failed".into()));
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_config(tmp: &std::path::Path, relative_path: &str, content: &str) -> PathBuf {
        let full = tmp.join(relative_path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, content).unwrap();
        full
    }

    #[test]
    fn adds_trytet_to_empty_config() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_config(
            tmp.path(),
            "Library/Application Support/Claude/claude_desktop_config.json",
            r#"{"preferences":{"theme":"dark"}}"#,
        );

        let results = register_mcp("/usr/local/bin/tet", &[("Claude Desktop", path.clone())]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "added");

        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"]["trytet"]["command"],
            "/usr/local/bin/tet"
        );
        assert_eq!(config["mcpServers"]["trytet"]["args"][0], "mcp");
        assert_eq!(config["preferences"]["theme"], "dark");
    }

    #[test]
    fn preserves_existing_mcp_servers() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_config(
            tmp.path(),
            "Library/Application Support/Claude/claude_desktop_config.json",
            r#"{"mcpServers":{"other-tool":{"command":"/usr/bin/other","args":[]}},"preferences":{}}"#,
        );

        let results = register_mcp("/opt/tet/bin/tet", &[("Claude Desktop", path.clone())]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "added");

        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"]["other-tool"]["command"],
            "/usr/bin/other"
        );
        assert_eq!(
            config["mcpServers"]["trytet"]["command"],
            "/opt/tet/bin/tet"
        );
    }

    #[test]
    fn detects_already_configured() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_config(
            tmp.path(),
            "Library/Application Support/Claude/claude_desktop_config.json",
            r#"{"mcpServers":{"trytet":{"command":"/usr/bin/tet","args":["mcp"]}}}"#,
        );

        let results = register_mcp("/usr/bin/tet", &[("Claude Desktop", path.clone())]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "already_configured");

        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config["mcpServers"]["trytet"]["command"], "/usr/bin/tet");
        assert_eq!(config["mcpServers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn skips_missing_config_files() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nonexistent").join("config.json");
        let results = register_mcp("/usr/bin/tet", &[("Claude Desktop", missing)]);
        assert!(results.is_empty());
    }

    #[test]
    fn handles_multiple_editors() {
        let tmp = tempfile::tempdir().unwrap();
        let claude = write_config(
            tmp.path(),
            "Library/Application Support/Claude/claude_desktop_config.json",
            r#"{"preferences":{}}"#,
        );
        let cursor = write_config(tmp.path(), ".cursor/mcp.json", r#"{"mcpServers":{}}"#);

        let results = register_mcp(
            "/usr/local/bin/tet",
            &[
                ("Claude Desktop", claude.clone()),
                ("Cursor", cursor.clone()),
            ],
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1, "added");
        assert_eq!(results[1].1, "added");

        for path in &[claude, cursor] {
            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
            assert_eq!(
                config["mcpServers"]["trytet"]["command"],
                "/usr/local/bin/tet"
            );
        }
    }
}
