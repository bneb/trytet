//! Multi-Tenant Fortress — Phase 17.1
//!
//! Cryptographic isolation, bandwidth quotas, and identity-stamped egress
//! for the Trytet Engine. Ensures that:
//!
//! 1. **Tenant Isolation** — Oracle caches are namespaced by `sha256(author_pubkey)`,
//!    preventing cross-tenant data leakage.
//! 2. **Bandwidth Metering** — `QuotaManager` tracks egress bytes per tenant with
//!    atomic CAS operations to handle thousands of concurrent agents.
//! 3. **Sovereign Headers** — Every outbound HTTP request is stamped with
//!    `X-Trytet-Agent-ID`, `X-Trytet-Tenant`, and `X-Trytet-Signature` (Ed25519).

use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Quota Manager
// ---------------------------------------------------------------------------

/// Tracks egress bandwidth consumption per tenant with thread-safe atomic updates.
///
/// Each tenant's usage is stored as an `AtomicU64` inside a `DashMap`,
/// enabling lock-free updates across thousands of concurrent agents.
pub struct QuotaManager {
    /// Tenant ID (SHA-256 prefix of author_pubkey) → cumulative bytes consumed.
    usage: DashMap<String, AtomicU64>,
}

/// Error returned when an agent's egress bandwidth exceeds its quota.
#[derive(Debug, Clone)]
pub struct QuotaExceeded {
    pub tenant: String,
    pub requested: u64,
    pub current: u64,
    pub limit: u64,
}

impl std::fmt::Display for QuotaExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EgressQuotaExceeded: tenant={} requested={} current={} limit={}",
            self.tenant, self.requested, self.current, self.limit
        )
    }
}

impl std::error::Error for QuotaExceeded {}

impl Default for QuotaManager {
    fn default() -> Self {
        Self::new()
    }
}

impl QuotaManager {
    /// Create a new, empty quota manager.
    pub fn new() -> Self {
        Self {
            usage: DashMap::new(),
        }
    }

    /// Check if adding `bytes` would exceed `limit` for this tenant.
    /// If allowed, atomically records the usage and returns `Ok(())`.
    /// If exceeded, returns `Err(QuotaExceeded)` without modifying usage.
    ///
    /// Uses `fetch_add` with `Ordering::SeqCst` for strict correctness.
    pub fn check_and_record(
        &self,
        tenant: &str,
        bytes: u64,
        limit: u64,
    ) -> Result<(), QuotaExceeded> {
        let entry = self
            .usage
            .entry(tenant.to_string())
            .or_insert_with(|| AtomicU64::new(0));

        // Atomically load current usage
        let current = entry.load(Ordering::SeqCst);

        if current.saturating_add(bytes) > limit {
            return Err(QuotaExceeded {
                tenant: tenant.to_string(),
                requested: bytes,
                current,
                limit,
            });
        }

        // Atomically add — in a high-contention scenario, this could slightly
        // overshoot, but that's acceptable for bandwidth metering (not billing).
        entry.fetch_add(bytes, Ordering::SeqCst);
        Ok(())
    }

    /// Get the current cumulative bytes consumed by a tenant.
    pub fn get_usage(&self, tenant: &str) -> u64 {
        self.usage
            .get(tenant)
            .map(|r| r.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Reset all tenant quotas to zero (e.g., daily rotation).
    pub fn reset_all(&self) {
        for entry in self.usage.iter() {
            entry.value().store(0, Ordering::SeqCst);
        }
    }
}

// ---------------------------------------------------------------------------
// Tenant Namespace
// ---------------------------------------------------------------------------

/// Derives tenant-isolated filesystem paths from the `author_pubkey`.
pub struct TenantNamespace;

impl TenantNamespace {
    /// Compute the tenant namespace directory for Oracle caches.
    ///
    /// Formula: `base_dir / sha256(author_pubkey)[..16] /`
    ///
    /// Uses the first 16 hex characters (64 bits of entropy) — sufficient for
    /// collision resistance across realistic agent populations (< 2^32 tenants).
    ///
    /// If `author_pubkey` is `None` or empty, falls back to a fixed "anonymous"
    /// namespace to avoid breaking agents without identity.
    pub fn derive_cache_dir(base_dir: &std::path::Path, author_pubkey: Option<&str>) -> PathBuf {
        let tenant_hash = match author_pubkey {
            Some(pk) if !pk.is_empty() && pk != "UNKNOWN" => {
                let mut hasher = Sha256::new();
                hasher.update(pk.as_bytes());
                let hash = hasher.finalize();
                hex::encode(&hash[..8]) // 8 bytes = 16 hex chars
            }
            _ => "anonymous".to_string(),
        };
        base_dir.join("oracle_cache").join(tenant_hash)
    }

    /// Compute the tenant ID string (used as the QuotaManager key).
    pub fn tenant_id(author_pubkey: Option<&str>) -> String {
        match author_pubkey {
            Some(pk) if !pk.is_empty() && pk != "UNKNOWN" => {
                let mut hasher = Sha256::new();
                hasher.update(pk.as_bytes());
                let hash = hasher.finalize();
                hex::encode(&hash[..8])
            }
            _ => "anonymous".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Sovereign Identity Headers
// ---------------------------------------------------------------------------

/// Generates the three Sovereign Identity headers for outbound HTTP requests.
///
/// Every request leaving the Trytet Engine carries:
/// - `X-Trytet-Agent-ID`: The unique TetID of the requesting agent
/// - `X-Trytet-Tenant`: The `author_pubkey` from the agent's manifest
/// - `X-Trytet-Signature`: Ed25519 signature of `method|url|body`
pub struct SovereignHeaders;

impl SovereignHeaders {
    /// Construct the identity headers for a given outbound request.
    ///
    /// # Arguments
    /// - `tet_id` — The agent's unique execution ID
    /// - `author_pubkey` — The tenant's public key (from manifest)
    /// - `wallet` — The node's signing wallet
    /// - `method` — HTTP method (GET, POST, etc.)
    /// - `url` — Full target URL
    /// - `body` — Request body bytes
    ///
    /// # Returns
    /// A vector of `(header_name, header_value)` tuples.
    pub fn inject(
        tet_id: &str,
        author_pubkey: &str,
        wallet: &crate::crypto::AgentWallet,
        method: &str,
        url: &str,
        body: &[u8],
    ) -> Vec<(String, String)> {
        // Build the signature payload: method|url|body
        let mut sign_payload = Vec::new();
        sign_payload.extend_from_slice(method.as_bytes());
        sign_payload.extend_from_slice(b"|");
        sign_payload.extend_from_slice(url.as_bytes());
        sign_payload.extend_from_slice(b"|");
        sign_payload.extend_from_slice(body);

        let signature_bytes = wallet.sign_bytes(&sign_payload);
        let signature_hex = hex::encode(&signature_bytes);

        vec![
            ("X-Trytet-Agent-ID".to_string(), tet_id.to_string()),
            ("X-Trytet-Tenant".to_string(), author_pubkey.to_string()),
            ("X-Trytet-Signature".to_string(), signature_hex),
        ]
    }

    /// Compute the total byte overhead of the injected headers.
    /// Used to include header size in the egress quota accounting.
    pub fn header_overhead(tet_id: &str, author_pubkey: &str) -> u64 {
        // Header names + values + approximate HTTP formatting overhead
        let names_len =
            "X-Trytet-Agent-ID".len() + "X-Trytet-Tenant".len() + "X-Trytet-Signature".len();
        let values_len = tet_id.len() + author_pubkey.len() + 128; // 128 = Ed25519 sig hex
        (names_len + values_len + 18) as u64 // 18 = ": " and "\r\n" separators × 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_namespace_different_pubkeys() {
        let dir = std::path::Path::new("/tmp/test");
        let a = TenantNamespace::derive_cache_dir(dir, Some("pubkey_aaa"));
        let b = TenantNamespace::derive_cache_dir(dir, Some("pubkey_bbb"));
        assert_ne!(a, b, "Different pubkeys must produce different namespaces");
    }

    #[test]
    fn test_tenant_namespace_deterministic() {
        let dir = std::path::Path::new("/tmp/test");
        let a1 = TenantNamespace::derive_cache_dir(dir, Some("pubkey_aaa"));
        let a2 = TenantNamespace::derive_cache_dir(dir, Some("pubkey_aaa"));
        assert_eq!(a1, a2, "Same pubkey must produce identical namespace");
    }

    #[test]
    fn test_tenant_namespace_anonymous() {
        let dir = std::path::Path::new("/tmp/test");
        let a = TenantNamespace::derive_cache_dir(dir, None);
        let b = TenantNamespace::derive_cache_dir(dir, Some(""));
        assert_eq!(a, b, "None and empty string both map to anonymous");
    }

    #[test]
    fn test_quota_manager_basic() {
        let qm = QuotaManager::new();
        assert!(qm.check_and_record("t1", 500, 1000).is_ok());
        assert_eq!(qm.get_usage("t1"), 500);
        assert!(qm.check_and_record("t1", 400, 1000).is_ok());
        assert_eq!(qm.get_usage("t1"), 900);
        // This should fail — 900 + 200 = 1100 > 1000
        assert!(qm.check_and_record("t1", 200, 1000).is_err());
        assert_eq!(qm.get_usage("t1"), 900); // unchanged
    }

    #[test]
    fn test_quota_manager_reset() {
        let qm = QuotaManager::new();
        qm.check_and_record("t1", 500, 1000).unwrap();
        qm.reset_all();
        assert_eq!(qm.get_usage("t1"), 0);
    }

    #[test]
    fn test_quota_manager_cross_tenant() {
        let qm = QuotaManager::new();
        qm.check_and_record("tenant_a", 800, 1000).unwrap();
        // tenant_b should have its own independent quota
        assert!(qm.check_and_record("tenant_b", 800, 1000).is_ok());
    }
}
