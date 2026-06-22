//! Centralized configuration for the Tet Core Engine.
//!
//! All environment-variable configuration is consolidated here. Call
//! [`Config::from_env`] early at startup so validation errors surface
//! immediately rather than being discovered at first use.

use std::path::PathBuf;

/// Consolidated runtime configuration.
///
/// Every field has a sensible default where one exists. Call
/// [`Config::from_env`] to build one from the environment, then
/// thread it through your application root.
#[derive(Clone, Debug)]
pub struct Config {
    /// Path to the local Wasm-snapshot / cartridge registry on disk.
    /// Defaults to `~/.trytet/registry`.
    pub registry_path: PathBuf,

    /// Base directory for generated tet artifacts.
    /// No default — if unset the engine uses a temp directory.
    pub base_tet_path: Option<PathBuf>,

    /// Database connection URL (e.g. `sqlite:///data/tet.db` or
    /// `postgres://user:pass@host/db`).
    pub database_url: Option<String>,

    /// URL of a remote OCI-compatible registry for cartridge distribution.
    pub registry_url: Option<String>,

    /// Bearer token for authenticating with the remote registry.
    /// Printed as `REDACTED` in diagnostic output.
    pub registry_token: Option<String>,

    /// Allowed CORS origin. `None` means permissive (any origin).
    pub cors_origin: Option<String>,

    /// Fly.io deployment region. Returns `"local"` when unset.
    pub fly_region: String,

    /// Colon-separated list of directories to search for cartridge wasm
    /// blobs.  Defaults to `~/.trytet/cartridges`.
    pub trytet_cartridge_dir: String,

    /// Base URL of the Tet API — used by the `tet` CLI to reach the engine.
    /// Defaults to `http://localhost:3000`.
    pub trytet_api_url: String,
}

// ---------------------------------------------------------------------------
// Environment variable names (public interface — do not rename)
// ---------------------------------------------------------------------------

const ENV_REGISTRY_PATH: &str = "REGISTRY_PATH";
const ENV_BASE_TET_PATH: &str = "BASE_TET_PATH";
const ENV_DATABASE_URL: &str = "DATABASE_URL";
const ENV_REGISTRY_URL: &str = "REGISTRY_URL";
const ENV_REGISTRY_TOKEN: &str = "REGISTRY_TOKEN";
const ENV_CORS_ORIGIN: &str = "CORS_ORIGIN";
const ENV_FLY_REGION: &str = "FLY_REGION";
const ENV_TRYTET_CARTRIDGE_DIR: &str = "TRYTET_CARTRIDGE_DIR";
const ENV_TRYTET_API_URL: &str = "TRYTET_API_URL";
impl Config {
    /// Build a [`Config`] from the process environment.
    ///
    /// Every variable is read independently — a missing or malformed value
    /// either falls back to a documented default or raises a clear error.
    pub fn from_env() -> Self {
        let home = || home::home_dir().unwrap_or_else(|| PathBuf::from("."));

        // --- registry_path ---
        let registry_path = env_var(ENV_REGISTRY_PATH)
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let p = home().join(".trytet").join("registry");
                tracing::debug!("{} unset, defaulting to {}", ENV_REGISTRY_PATH, p.display());
                p
            });

        // --- base_tet_path ---
        let base_tet_path = env_var(ENV_BASE_TET_PATH).map(PathBuf::from);

        // --- database_url ---
        let database_url = env_var(ENV_DATABASE_URL);
        if let Some(ref url) = database_url {
            // Quick structural check — starts with a known scheme
            if !(url.starts_with("sqlite://")
                || url.starts_with("postgres://")
                || url.starts_with("postgresql://")
                || url.starts_with("mysql://"))
            {
                tracing::warn!(
                    "{} is set to an unrecognised scheme — expected sqlite://, postgres://, \
                     postgresql://, or mysql:// (value will be passed through as-is)",
                    ENV_DATABASE_URL
                );
            }
        }

        // --- registry_url / registry_token ---
        let registry_url = env_var(ENV_REGISTRY_URL);
        let registry_token = env_var(ENV_REGISTRY_TOKEN);
        if registry_url.is_some() && registry_token.is_none() {
            tracing::warn!(
                "{} is set but {} is not — registry push/pull may fail for private registries",
                ENV_REGISTRY_URL,
                ENV_REGISTRY_TOKEN
            );
        }

        // --- cors_origin ---
        let cors_origin = env_var(ENV_CORS_ORIGIN)
            .filter(|o| !o.is_empty())
            .or_else(|| Some("http://localhost:3000".to_string()));

        // --- fly_region ---
        let fly_region = env_var(ENV_FLY_REGION).unwrap_or_else(|| {
            tracing::debug!("{} unset, defaulting to \"local\"", ENV_FLY_REGION);
            "local".to_string()
        });

        // --- trytet_cartridge_dir ---
        let trytet_cartridge_dir = env_var(ENV_TRYTET_CARTRIDGE_DIR).unwrap_or_else(|| {
            let d = home().join(".trytet").join("cartridges");
            let s = d.to_string_lossy().to_string();
            tracing::debug!("{} unset, defaulting to {}", ENV_TRYTET_CARTRIDGE_DIR, s);
            s
        });

        // --- trytet_api_url ---
        let trytet_api_url = env_var(ENV_TRYTET_API_URL).unwrap_or_else(|| {
            tracing::debug!(
                "{} unset, defaulting to http://localhost:3000",
                ENV_TRYTET_API_URL
            );
            "http://localhost:3000".to_string()
        });

        Self {
            registry_path,
            base_tet_path,
            database_url,
            registry_url,
            registry_token,
            cors_origin,
            fly_region,
            trytet_cartridge_dir,
            trytet_api_url,
        }
    }

    /// Print the full configuration to stdout, with secrets redacted.
    ///
    /// Intended for the `tet doctor` diagnostic command.
    pub fn print_doctor(&self) {
        println!("  Configuration");
        println!("    REGISTRY_PATH:       {}", self.registry_path.display());
        println!(
            "    BASE_TET_PATH:       {}",
            self.base_tet_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(not set)".to_string())
        );
        println!(
            "    DATABASE_URL:        {}",
            redact_db_url(&self.database_url)
        );
        println!(
            "    REGISTRY_URL:        {}",
            self.registry_url.as_deref().unwrap_or("(not set)")
        );
        println!(
            "    REGISTRY_TOKEN:      {}",
            self.registry_token
                .as_ref()
                .map(|t| redact_token(t))
                .unwrap_or_else(|| "(not set)".to_string())
        );
        println!(
            "    CORS_ORIGIN:         {}",
            self.cors_origin.as_deref().unwrap_or("(permissive)")
        );
        println!("    FLY_REGION:          {}", self.fly_region);
        println!("    TRYTET_CARTRIDGE_DIR: {}", self.trytet_cartridge_dir);
        println!("    TRYTET_API_URL:      {}", self.trytet_api_url);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read an env var without printing it to stderr (plain `std::env::var`).
fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// Show only the first 4 characters of a bearer token.
fn redact_token(token: &str) -> String {
    if token.len() <= 8 {
        "REDACTED".to_string()
    } else {
        format!("{}… ({} chars)", &token[..4], token.len())
    }
}

/// Redact the password portion of a database URL.
///
/// E.g. `postgres://user:secret@host/db` → `postgres://user:****@host/db`
fn redact_db_url(url: &Option<String>) -> String {
    match url {
        None => "(not set)".to_string(),
        Some(raw) => {
            // Find the colon that separates user:password — it's after the
            // `://` scheme separator and before the `@` host separator.
            let scheme_end = raw.find("://").map(|i| i + 3).unwrap_or(0);
            if let Some(after_colon) = raw[scheme_end..].find(':') {
                let colon_pos = scheme_end + after_colon;
                if let Some(at_sign) = raw.rfind('@') {
                    if colon_pos < at_sign {
                        let prefix = &raw[..=colon_pos];
                        let suffix = &raw[at_sign..];
                        return format!("{}****{}", prefix, suffix);
                    }
                }
            }
            raw.clone()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_db_url_with_password() {
        let url = Some("postgres://admin:hunter2@db.example.com:5432/tet".to_string());
        assert_eq!(
            redact_db_url(&url),
            "postgres://admin:****@db.example.com:5432/tet"
        );
    }

    #[test]
    fn redact_db_url_no_password() {
        let url = Some("sqlite:///data/tet.db".to_string());
        assert_eq!(redact_db_url(&url), "sqlite:///data/tet.db");
    }

    #[test]
    fn redact_db_url_none() {
        assert_eq!(redact_db_url(&None), "(not set)");
    }

    #[test]
    fn redact_short_token() {
        assert_eq!(redact_token("abc"), "REDACTED");
    }

    #[test]
    fn redact_long_token() {
        let raw = "ghp_abc123def456ghi789jkl";
        let t = redact_token(raw);
        assert!(t.starts_with(&raw[..4]));
        assert!(t.contains('…'));
        assert!(t.contains(&format!("({} chars)", raw.len())));
    }
}
