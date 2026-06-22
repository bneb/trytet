use axum::{
    body,
    extract::{Path, Request, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, head, post, put},
    Router,
};
use tet_core::builder::TetArtifact;
use tet_core::models::manifest::AgentManifest;
use tet_core::registry::oci::{OciClient, MEDIA_TYPE_MANIFEST};

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

use sha2::Digest;

struct MockRegistryState {
    blobs: Mutex<HashMap<String, Vec<u8>>>,
    manifests: Mutex<HashMap<String, Vec<u8>>>,
    require_auth: bool,
}

#[tokio::test]
async fn test_manifest_protocol() {
    let state = Arc::new(MockRegistryState {
        blobs: Mutex::new(HashMap::new()),
        manifests: Mutex::new(HashMap::new()),
        require_auth: false,
    });

    let app = Router::new()
        .route("/v2/{name}/blobs/uploads/", post(start_upload))
        .route("/v2/{name}/blobs/uploads/{id}", put(finish_upload))
        .route("/v2/{name}/blobs/{digest}", head(check_blob).get(get_blob))
        .route("/v2/{name}/manifests/{tag}", put(put_manifest))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let registry_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Setup dummy artifact
    let temp = TempDir::new().unwrap();
    let wasm_path = temp.path().join("test.wasm");
    fs::write(&wasm_path, b"wasm content").unwrap();

    let artifact = TetArtifact {
        manifest: serde_json::from_str(
            r#"{
            "metadata": {"name": "test-agent", "version": "1.0.0", "author_pubkey": "pk"},
            "constraints": {"max_memory_pages": 100, "fuel_limit": 1000},
            "permissions": {"can_egress": [], "can_persist": true, "can_teleport": true}
        }"#,
        )
        .unwrap(),
        blueprint_wasm: b"wasm content".to_vec(),
        vfs_zstd: b"vfs content".to_vec(),
        signature: b"sig content".to_vec(),
    };

    let client = OciClient::new(registry_url, None);
    client
        .push(&artifact, "agent:v1")
        .await
        .expect("Push failed");

    // Assert manifest received with correct media type
    let rests = state.manifests.lock().await;
    assert!(
        rests.contains_key("agent/v1"),
        "Manifest not found in {:?}",
        rests.keys()
    );
}

#[tokio::test]
async fn test_corruption_check() {
    let state = Arc::new(MockRegistryState {
        blobs: Mutex::new(HashMap::new()),
        manifests: Mutex::new(HashMap::new()),
        require_auth: false,
    });

    // Predeterministically seed the registry with corrupted data
    let wasm_data = b"correct wasm".to_vec();
    let corrupted_wasm = b"corrupted wasm".to_vec();
    let wasm_digest = format!("sha256:{}", hex::encode(sha2::Sha256::digest(&wasm_data)));

    state
        .blobs
        .lock()
        .await
        .insert(wasm_digest.clone(), corrupted_wasm);

    // Seed manifest
    let manifest_json = format!(
        r#"{{
        "schemaVersion": 2,
        "mediaType": "{}",
        "config": {{"mediaType": "application/vnd.trytet.config.v1+json", "digest": "sha256:config", "size": 10}},
        "layers": [
            {{"mediaType": "application/vnd.trytet.layer.v1.wasm", "digest": "{}", "size": 12}},
            {{"mediaType": "application/vnd.trytet.layer.v1.tar+zstd", "digest": "sha256:vfs", "size": 10}},
            {{"mediaType": "application/vnd.trytet.layer.v1.signature", "digest": "sha256:sig", "size": 3}}
        ]
    }}"#,
        MEDIA_TYPE_MANIFEST, wasm_digest
    );
    state
        .manifests
        .lock()
        .await
        .insert("agent/latest".to_string(), manifest_json.into_bytes());

    // Also seed other blobs to avoid 404
    let dummy_manifest = AgentManifest {
        metadata: tet_core::models::manifest::Metadata {
            name: "corrupt-agent".to_string(),
            version: "1.0.0".to_string(),
            author_pubkey: Some("pk".to_string()),
        },
        constraints: tet_core::models::manifest::ResourceConstraints {
            max_memory_pages: 100,
            fuel_limit: 1000,
            max_egress_bytes: 1_000_000,
        },
        permissions: tet_core::models::manifest::CapabilityPolicy {
            can_egress: vec![],
            can_persist: true,
            can_teleport: true,
            is_genesis_factory: false,
            can_fork: false,
        },
    };
    let dummy_manifest_json = serde_json::to_vec(&dummy_manifest).unwrap();
    state
        .blobs
        .lock()
        .await
        .insert("sha256:config".to_string(), dummy_manifest_json);
    state
        .blobs
        .lock()
        .await
        .insert("sha256:vfs".to_string(), b"vfs".to_vec());
    state
        .blobs
        .lock()
        .await
        .insert("sha256:sig".to_string(), b"sig".to_vec());

    let app = Router::new()
        .route("/v2/{name}/manifests/{tag}", get(get_manifest))
        .route("/v2/{name}/blobs/{digest}", get(get_blob))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let registry_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = OciClient::new(registry_url, None);
    let result = client.pull("agent:latest").await;

    assert!(
        result.is_err(),
        "Pull should have failed due to digest mismatch"
    );
    assert!(result.unwrap_err().to_string().contains("Digest mismatch"));
}

#[tokio::test]
async fn test_token_flow_auth() {
    let state = Arc::new(MockRegistryState {
        blobs: Mutex::new(HashMap::new()),
        manifests: Mutex::new(HashMap::new()),
        require_auth: true,
    });

    let app = Router::new()
        .route("/v2/{name}/blobs/uploads/", post(start_upload))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let registry_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let artifact = TetArtifact {
        manifest: AgentManifest {
            metadata: tet_core::models::manifest::Metadata {
                name: "auth-test".to_string(),
                version: "1.0.0".to_string(),
                author_pubkey: Some("pk".to_string()),
            },
            constraints: tet_core::models::manifest::ResourceConstraints {
                max_memory_pages: 100,
                fuel_limit: 1000,
                max_egress_bytes: 1_000_000,
            },
            permissions: tet_core::models::manifest::CapabilityPolicy {
                can_egress: vec![],
                can_persist: true,
                can_teleport: true,
                is_genesis_factory: false,
                can_fork: false,
            },
        },
        blueprint_wasm: vec![],
        vfs_zstd: vec![],
        signature: vec![],
    };

    let client = OciClient::new(registry_url, None); // No token
    let result = client.push(&artifact, "agent:v1").await;

    assert!(
        result.is_err(),
        "Push should have failed due to missing token"
    );
    assert!(result.unwrap_err().to_string().contains("401"));
}

// Handler Implementation for Mock Registry
// NOTE: body extractor must be LAST in axum
async fn start_upload(
    State(state): State<Arc<MockRegistryState>>,
    req: Request,
) -> impl IntoResponse {
    if state.require_auth && req.headers().get("Authorization").is_none() {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    (
        StatusCode::ACCEPTED,
        [(header::LOCATION, "/v2/test/blobs/uploads/123")],
    )
        .into_response()
}

async fn finish_upload(
    State(_state): State<Arc<MockRegistryState>>,
    Path((_name, _id)): Path<(String, String)>,
    _body: body::Bytes,
) -> impl IntoResponse {
    StatusCode::CREATED
}

async fn check_blob(
    State(state): State<Arc<MockRegistryState>>,
    Path((_name, digest)): Path<(String, String)>,
) -> impl IntoResponse {
    let blobs = state.blobs.lock().await;
    if blobs.contains_key(&digest) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn get_blob(
    State(state): State<Arc<MockRegistryState>>,
    Path((_name, digest)): Path<(String, String)>,
) -> impl IntoResponse {
    let blobs = state.blobs.lock().await;
    if let Some(data) = blobs.get(&digest) {
        (StatusCode::OK, data.clone()).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn put_manifest(
    State(state): State<Arc<MockRegistryState>>,
    Path((_name, tag)): Path<(String, String)>,
    body: body::Bytes,
) -> impl IntoResponse {
    state
        .manifests
        .lock()
        .await
        .insert(format!("{}/{}", _name, tag), body.to_vec());
    StatusCode::CREATED
}

async fn get_manifest(
    State(state): State<Arc<MockRegistryState>>,
    Path((_name, tag)): Path<(String, String)>,
) -> impl IntoResponse {
    let manifests = state.manifests.lock().await;
    if let Some(data) = manifests.get(&tag) {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, MEDIA_TYPE_MANIFEST)],
            data.clone(),
        )
            .into_response()
    } else if let Some(data) = manifests.get(&format!("{}/{}", _name, tag)) {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, MEDIA_TYPE_MANIFEST)],
            data.clone(),
        )
            .into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
