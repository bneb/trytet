//! The Sovereign Oracle
//!
//! Bridges the Trytet Zero-Trust Sandbox to the Legacy Internet via
//! deterministic proxy gateways. Enforces Egress domain whitelists
//! and exposes Ingress listening surfaces mapped to internal aliases.

use serde::{Deserialize, Serialize};

/// Defines the security boundaries for a Tet's external internet access.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EgressPolicy {
    /// List of exact host domains the agent is allowed to access (e.g., "api.openai.com").
    pub allowed_domains: Vec<String>,

    /// The maximum number of deterministic network bytes this Tet can transmit/receive
    /// across all combined egress calls per execution lifecycle.
    pub max_daily_bytes: u64,

    /// Strict TLS enforcement. If `true`, the `fetch` host function will outright
    /// reject `http://` prefix requests to prevent plaintext leakage.
    pub require_https: bool,
}

/// A mapping projecting a public Legacy HTTP path into a specific Trytet Mesh Alias.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IngressRoute {
    /// The public facing suffix (e.g., `/v1/chat`).
    pub public_path: String,

    /// The registered internal Tet Mesh Alias (e.g., `chat-agent`).
    pub target_alias: String,

    /// Which HTTP methods are permitted to bridge.
    pub method_filter: Vec<String>,
}
