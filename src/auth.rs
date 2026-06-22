//! API key management and usage tracking.
//!
//! Provides `KeyStore` for creating, validating, and revoking API keys,
//! and an axum middleware (`require_api_key`) for protecting routes.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// An API key record.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiKey {
    pub prefix: String, // First 8 chars for identification
    pub hash: String,   // SHA-256 of the full key
    pub label: String,  // Human-readable name
    pub created: u64,   // Unix timestamp
    pub active: bool,
}

impl ApiKey {
    pub fn new(label: String) -> (Self, String) {
        let raw = format!("tet_{}", uuid::Uuid::new_v4());
        let mut hasher = Sha256::new();
        hasher.update(raw.as_bytes());
        let hash = hex::encode(hasher.finalize());
        let prefix = raw[..12].to_string();
        (
            Self {
                prefix,
                hash,
                label,
                created: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                active: true,
            },
            raw,
        )
    }
}

/// Thread-safe key store and usage tracker.
pub struct KeyStore {
    keys: DashMap<String, ApiKey>,     // hash -> key record
    usage: DashMap<String, AtomicU64>, // hash -> invocation count
}

impl KeyStore {
    pub fn new() -> Self {
        Self {
            keys: DashMap::new(),
            usage: DashMap::new(),
        }
    }

    /// Returns true if the store has any active keys (used for boot-key logic).
    pub fn has_keys(&self) -> bool {
        self.keys.iter().any(|e| e.active)
    }

    /// Create a new API key. Returns the raw key (shown once).
    pub fn create_key(&self, label: String) -> String {
        let (key, raw) = ApiKey::new(label);
        let prefix = key.prefix.clone();
        self.keys.insert(key.hash.clone(), key);
        self.usage.insert(prefix, AtomicU64::new(0));
        raw
    }

    /// Validate an API key. Returns the key hash if valid.
    pub fn validate(&self, raw: &str) -> Option<String> {
        let mut hasher = Sha256::new();
        hasher.update(raw.as_bytes());
        let hash = hex::encode(hasher.finalize());
        self.keys.get(&hash).filter(|k| k.active).map(|_| {
            self.usage
                .entry(hash.clone())
                .or_default()
                .fetch_add(1, Ordering::Relaxed);
            hash
        })
    }

    /// Revoke an API key by prefix.
    pub fn revoke(&self, prefix: &str) -> bool {
        self.keys.iter_mut().any(|mut entry| {
            if entry.prefix == prefix && entry.active {
                entry.active = false;
                true
            } else {
                false
            }
        })
    }

    /// List active API keys.
    pub fn list(&self) -> Vec<(String, u64, String)> {
        self.keys
            .iter()
            .filter(|e| e.active)
            .map(|e| {
                let count = self
                    .usage
                    .get(e.key())
                    .map(|u| u.load(Ordering::Relaxed))
                    .unwrap_or(0);
                (e.prefix.clone(), count, e.label.clone())
            })
            .collect()
    }
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

/// Axum middleware that requires a valid API key for protected routes.
///
/// Reads the key from `Authorization: Bearer <key>` or `X-API-Key: <key>`.
/// Returns 401 if the key is missing or invalid.
///
/// Unauthenticated paths (health, console, MCP, ingress proxy) are not
/// subject to this middleware — apply it selectively.
pub async fn require_api_key(
    State(store): State<Arc<KeyStore>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Boot-key mode: if the store is empty, allow all requests through.
    // The server binary auto-creates a boot key on first run; until then
    // (or in tests with an empty store), auth is skipped.
    if !store.has_keys() {
        return Ok(next.run(req).await);
    }

    let key = extract_api_key(&req);
    match key {
        Some(k) if store.validate(&k).is_some() => Ok(next.run(req).await),
        _ => {
            let body = serde_json::json!({
                "error": "unauthorized",
                "error_type": "AuthenticationRequired",
                "message": "A valid API key is required. Pass it via Authorization: Bearer <key> or X-API-Key header."
            });
            Ok(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&body).unwrap_or_default(),
                ))
                .expect("build auth error response"))
        }
    }
}

fn extract_api_key(req: &Request) -> Option<String> {
    // 1. X-API-Key header (simplest for scripting)
    if let Some(key) = req.headers().get("x-api-key") {
        if let Ok(s) = key.to_str() {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    // 2. Authorization: Bearer <key>
    if let Some(auth) = req.headers().get("authorization") {
        if let Ok(s) = auth.to_str() {
            if let Some(key) = s.strip_prefix("Bearer ") {
                let trimmed = key.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}
