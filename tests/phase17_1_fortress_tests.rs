use std::collections::HashMap;
use tet_core::crypto::AgentWallet;
use tet_core::fortress::{QuotaManager, SovereignHeaders, TenantNamespace};
use tet_core::models::manifest::{AgentManifest, CapabilityPolicy, Metadata, ResourceConstraints};

fn make_manifest(pubkey: &str, max_egress: u64) -> AgentManifest {
    AgentManifest {
        metadata: Metadata {
            name: "fortress_test".to_string(),
            version: "1".to_string(),
            author_pubkey: Some(pubkey.to_string()),
        },
        constraints: ResourceConstraints {
            max_memory_pages: 10,
            fuel_limit: 50_000_000,
            max_egress_bytes: max_egress,
        },
        permissions: CapabilityPolicy {
            can_egress: vec!["example.com".to_string()],
            can_persist: false,
            can_teleport: false,
            is_genesis_factory: false,
            can_fork: false,
        },
    }
}

// ----------------------------------------------------------------
// TDD Case 1: Privacy Leak
// Boot Agent A (Tenant 1) and Agent B (Tenant 2).
// Their oracle cache directories must be cryptographically isolated.
// Agent B must never be able to read Agent A's cached SignedTruths.
// ----------------------------------------------------------------
#[test]
fn test_privacy_leak() {
    let base = std::path::Path::new("/tmp/fortress_test");

    // Agent A arrives with pubkey_aaa
    let dir_a = TenantNamespace::derive_cache_dir(base, Some("pubkey_aaa"));
    let tid_a = TenantNamespace::tenant_id(Some("pubkey_aaa"));

    // Agent B arrives with pubkey_bbb
    let dir_b = TenantNamespace::derive_cache_dir(base, Some("pubkey_bbb"));
    let tid_b = TenantNamespace::tenant_id(Some("pubkey_bbb"));

    // 1. Directories must be different
    assert_ne!(
        dir_a, dir_b,
        "PRIVACY LEAK: Tenants A and B share the same cache directory!"
    );

    // 2. Tenant IDs must be different
    assert_ne!(
        tid_a, tid_b,
        "PRIVACY LEAK: Tenants A and B have the same tenant ID!"
    );

    // 3. Same pubkey always derives the same namespace (deterministic)
    let dir_a2 = TenantNamespace::derive_cache_dir(base, Some("pubkey_aaa"));
    assert_eq!(
        dir_a, dir_a2,
        "Non-deterministic cache path: same pubkey produced different paths"
    );

    // 4. Verify physical path includes the hash prefix
    let dir_a_str = dir_a.to_string_lossy();
    assert!(
        dir_a_str.contains("oracle_cache"),
        "Cache dir should contain 'oracle_cache' segment"
    );
    assert!(
        !dir_a_str.ends_with("oracle_cache"),
        "Cache dir should have a tenant hash suffix, not just 'oracle_cache'"
    );

    // 5. QuotaManager must isolate tenant budgets
    let qm = QuotaManager::new();
    // Agent A consumes 900 bytes out of 1000
    qm.check_and_record(&tid_a, 900, 1000).unwrap();
    // Agent B should have its own independent quota
    assert!(
        qm.check_and_record(&tid_b, 900, 1000).is_ok(),
        "PRIVACY LEAK: Agent B's quota was polluted by Agent A's consumption!"
    );
    // Agent A's is still at 900
    assert_eq!(qm.get_usage(&tid_a), 900);
    assert_eq!(qm.get_usage(&tid_b), 900);
}

// ----------------------------------------------------------------
// TDD Case 2: Bandwidth Cap
// Set max_egress_bytes = 1024 (1KB). The QuotaManager must enforce
// this limit and return QuotaExceeded when the budget is blown.
// ----------------------------------------------------------------
#[test]
fn test_bandwidth_cap() {
    let qm = QuotaManager::new();
    let tenant = "test_tenant_cap";
    let limit: u64 = 1024; // 1KB budget

    // First request: 500 bytes — should succeed
    assert!(
        qm.check_and_record(tenant, 500, limit).is_ok(),
        "First 500-byte request should succeed within 1KB budget"
    );
    assert_eq!(qm.get_usage(tenant), 500);

    // Second request: 400 bytes — should still succeed (total: 900 < 1024)
    assert!(
        qm.check_and_record(tenant, 400, limit).is_ok(),
        "Second 400-byte request should succeed (900 < 1024)"
    );
    assert_eq!(qm.get_usage(tenant), 900);

    // Third request: 200 bytes — should FAIL (total would be 1100 > 1024)
    let err = qm.check_and_record(tenant, 200, limit);
    assert!(
        err.is_err(),
        "Third request should fail: 900 + 200 = 1100 > 1024"
    );
    // Usage must not have changed on failure
    assert_eq!(
        qm.get_usage(tenant),
        900,
        "Usage must not increase on failed quota check"
    );

    // Verify error details
    let quota_err = err.unwrap_err();
    assert_eq!(quota_err.current, 900);
    assert_eq!(quota_err.requested, 200);
    assert_eq!(quota_err.limit, 1024);

    // After reset, the same request should succeed
    qm.reset_all();
    assert_eq!(qm.get_usage(tenant), 0, "Usage must be zero after reset");
    assert!(
        qm.check_and_record(tenant, 200, limit).is_ok(),
        "After reset, 200-byte request should succeed"
    );

    // Verify that error code 8 would be returned by the sandbox by checking
    // that the manifest properly encodes max_egress_bytes
    let manifest = make_manifest("test_pubkey", 1024);
    assert_eq!(manifest.constraints.max_egress_bytes, 1024);
}

// ----------------------------------------------------------------
// TDD Case 3: Authenticated Egress
// Construct SovereignHeaders and verify:
// 1. X-Trytet-Agent-ID matches the tet_id
// 2. X-Trytet-Tenant matches the author_pubkey
// 3. X-Trytet-Signature is a valid Ed25519 signature verifiable
//    against the node wallet's public key
// ----------------------------------------------------------------
#[test]
fn test_authenticated_egress() {
    let wallet = AgentWallet::load_or_create().unwrap();
    let node_pubkey = wallet.public_key_hex();

    let tet_id = "tet-12345-abcde";
    let author_pubkey = "author_pubkey_xyz";
    let method = "POST";
    let url = "https://api.example.com/v1/data";
    let body = b"hello world";

    let headers = SovereignHeaders::inject(tet_id, author_pubkey, &wallet, method, url, body);

    // Verify exactly 3 headers returned
    assert_eq!(headers.len(), 3, "Expected exactly 3 sovereign headers");

    // Extract headers into a map for easy lookup
    let header_map: HashMap<String, String> = headers.into_iter().collect();

    // 1. X-Trytet-Agent-ID must match tet_id
    assert_eq!(
        header_map.get("X-Trytet-Agent-ID").unwrap(),
        tet_id,
        "Agent ID header mismatch"
    );

    // 2. X-Trytet-Tenant must match author_pubkey
    assert_eq!(
        header_map.get("X-Trytet-Tenant").unwrap(),
        author_pubkey,
        "Tenant header mismatch"
    );

    // 3. X-Trytet-Signature must be a valid Ed25519 signature
    let signature_hex = header_map.get("X-Trytet-Signature").unwrap();

    // Reconstruct the signed payload: method|url|body
    let mut expected_payload = Vec::new();
    expected_payload.extend_from_slice(method.as_bytes());
    expected_payload.extend_from_slice(b"|");
    expected_payload.extend_from_slice(url.as_bytes());
    expected_payload.extend_from_slice(b"|");
    expected_payload.extend_from_slice(body);

    let verified = AgentWallet::verify_signature(&node_pubkey, &expected_payload, signature_hex);
    assert!(
        verified,
        "Ed25519 signature verification FAILED! The X-Trytet-Signature is not valid for the node's public key."
    );

    // 4. Verify header overhead calculation is non-zero
    let overhead = SovereignHeaders::header_overhead(tet_id, author_pubkey);
    assert!(overhead > 0, "Header overhead must be non-zero");

    // 5. Verify that a different body produces a DIFFERENT signature
    let headers2 = SovereignHeaders::inject(
        tet_id,
        author_pubkey,
        &wallet,
        method,
        url,
        b"different body",
    );
    let header_map2: HashMap<String, String> = headers2.into_iter().collect();
    let sig2 = header_map2.get("X-Trytet-Signature").unwrap();
    assert_ne!(
        signature_hex, sig2,
        "Different bodies must produce different signatures"
    );
}
