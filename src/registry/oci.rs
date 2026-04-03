use crate::builder::TetArtifact;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};

pub const MEDIA_TYPE_MANIFEST: &str = "application/vnd.trytet.manifest.v1+json";
pub const MEDIA_TYPE_CONFIG: &str = "application/vnd.trytet.config.v1+json";
pub const MEDIA_TYPE_LAYER_WASM: &str = "application/vnd.trytet.layer.v1.wasm";
pub const MEDIA_TYPE_LAYER_VFS: &str = "application/vnd.trytet.layer.v1.tar+zstd";

pub const MEDIA_TYPE_LAYER_SIGNATURE: &str = "application/vnd.trytet.layer.v1.signature";

#[derive(Debug, Serialize, Deserialize)]
pub struct OciManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub config: OciDescriptor,
    pub layers: Vec<OciDescriptor>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OciDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: usize,
}

pub struct OciClient {
    pub registry_url: String,
    pub token: Option<String>,
    pub http: reqwest::Client,
}

impl OciClient {
    pub fn new(registry_url: String, token: Option<String>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("User-Agent", reqwest::header::HeaderValue::from_static("tet-cli/1.0"));
        
        Self {
            registry_url,
            token,
            http: reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .unwrap_or_default(),
        }
    }

    fn get_auth_header(&self) -> Option<String> {
        self.token.as_ref().map(|t| format!("Bearer {}", t))
    }

    pub async fn push(&self, artifact: &TetArtifact, reference: &str) -> Result<()> {
        let (name, tag) = reference.split_once(':').unwrap_or((reference, "latest"));
        
        let wasm_digest = format!("sha256:{}", hex::encode(Sha256::digest(&artifact.blueprint_wasm)));
        let vfs_digest = format!("sha256:{}", hex::encode(Sha256::digest(&artifact.vfs_zstd)));
        let sig_digest = format!("sha256:{}", hex::encode(Sha256::digest(&artifact.signature)));
        
        let config_json = serde_json::to_vec(&artifact.manifest)?;
        let config_digest = format!("sha256:{}", hex::encode(Sha256::digest(&config_json)));

        // Upload Blobs
        self.upload_blob(name, &wasm_digest, &artifact.blueprint_wasm).await?;
        self.upload_blob(name, &vfs_digest, &artifact.vfs_zstd).await?;
        self.upload_blob(name, &sig_digest, &artifact.signature).await?;
        self.upload_blob(name, &config_digest, &config_json).await?;

        // Construct and Upload Manifest
        let manifest = OciManifest {
            schema_version: 2,
            media_type: MEDIA_TYPE_MANIFEST.to_string(),
            config: OciDescriptor {
                media_type: MEDIA_TYPE_CONFIG.to_string(),
                digest: config_digest,
                size: config_json.len(),
            },
            layers: vec![
                OciDescriptor {
                    media_type: MEDIA_TYPE_LAYER_WASM.to_string(),
                    digest: wasm_digest,
                    size: artifact.blueprint_wasm.len(),
                },
                OciDescriptor {
                    media_type: MEDIA_TYPE_LAYER_VFS.to_string(),
                    digest: vfs_digest,
                    size: artifact.vfs_zstd.len(),
                },
                OciDescriptor {
                    media_type: MEDIA_TYPE_LAYER_SIGNATURE.to_string(),
                    digest: sig_digest,
                    size: artifact.signature.len(),
                },
            ],
        };

        let manifest_json = serde_json::to_vec(&manifest)?;
        let url = format!("{}/v2/{}/manifests/{}", self.registry_url, name, tag);
        
        let mut req = self.http.put(&url)
            .header("Content-Type", MEDIA_TYPE_MANIFEST)
            .body(manifest_json);
            
        if let Some(auth) = self.get_auth_header() {
            req = req.header("Authorization", auth);
        }

        let res = req.send().await?;
        if !res.status().is_success() {
            return Err(anyhow!("Failed to push manifest: {} - {}", res.status(), res.text().await?));
        }

        Ok(())
    }

    async fn upload_blob(&self, name: &str, digest: &str, data: &[u8]) -> Result<()> {
        let check_url = format!("{}/v2/{}/blobs/{}", self.registry_url, name, digest);
        let mut check_req = self.http.head(&check_url);
        if let Some(auth) = self.get_auth_header() {
            check_req = check_req.header("Authorization", auth);
        }
        
        if check_req.send().await?.status().is_success() {
            return Ok(()); // Already exists
        }

        let upload_url = format!("{}/v2/{}/blobs/uploads/", self.registry_url, name);
        let mut start_req = self.http.post(&upload_url);
        if let Some(auth) = self.get_auth_header() {
            start_req = start_req.header("Authorization", auth);
        }
        
        let start_res = start_req.send().await?;
        if !start_res.status().is_success() {
            return Err(anyhow!("Failed to start blob upload: {} - {}", start_res.status(), start_res.text().await?));
        }

        let location = start_res.headers().get("Location")
            .ok_or_else(|| anyhow!("No location header in blob upload start"))?
            .to_str()?;
            
        let mut final_url = if location.starts_with('/') {
            format!("{}{}", self.registry_url, location)
        } else {
            location.to_string()
        };

        if final_url.contains('?') {
            final_url = format!("{}&digest={}", final_url, digest);
        } else {
            final_url = format!("{}?digest={}", final_url, digest);
        }

        let mut put_req = self.http.put(&final_url)
            .header("Content-Length", data.len())
            .header("Content-Type", "application/octet-stream")
            .body(data.to_vec());
            
        if let Some(auth) = self.get_auth_header() {
            put_req = put_req.header("Authorization", auth);
        }

        let put_res = put_req.send().await?;
        if !put_res.status().is_success() {
            return Err(anyhow!("Failed to upload blob: {} - {}", put_res.status(), put_res.text().await?));
        }

        Ok(())
    }

    pub async fn pull(&self, reference: &str) -> Result<TetArtifact> {
        let (name, tag) = reference.split_once(':').unwrap_or((reference, "latest"));
        
        let url = format!("{}/v2/{}/manifests/{}", self.registry_url, name, tag);
        let mut req = self.http.get(&url)
            .header("Accept", MEDIA_TYPE_MANIFEST);
            
        if let Some(auth) = self.get_auth_header() {
            req = req.header("Authorization", auth);
        }

        let res = req.send().await?;
        if !res.status().is_success() {
            return Err(anyhow!("Failed to pull manifest: {} - {}", res.status(), res.text().await?));
        }
        
        let manifest: OciManifest = res.json().await?;

        // Parallel fetch of all layers and config
        let config_digest = manifest.config.digest.clone();
        let wasm_desc = manifest.layers.iter().find(|l| l.media_type == MEDIA_TYPE_LAYER_WASM)
            .ok_or_else(|| anyhow!("Missing WASM layer"))?;
        let vfs_desc = manifest.layers.iter().find(|l| l.media_type == MEDIA_TYPE_LAYER_VFS)
            .ok_or_else(|| anyhow!("Missing VFS layer"))?;
        let sig_desc = manifest.layers.iter().find(|l| l.media_type == MEDIA_TYPE_LAYER_SIGNATURE)
            .ok_or_else(|| anyhow!("Missing Signature layer"))?;

        let wasm_digest = wasm_desc.digest.clone();
        let vfs_digest = vfs_desc.digest.clone();
        let sig_digest = sig_desc.digest.clone();

        let (config_res, wasm_res, vfs_res, sig_res) = tokio::join!(
            self.fetch_blob(name, &config_digest),
            self.fetch_blob(name, &wasm_digest),
            self.fetch_blob(name, &vfs_digest),
            self.fetch_blob(name, &sig_digest)
        );

        let config_data = config_res?;
        let wasm_bytes = wasm_res?;
        let vfs_bytes = vfs_res?;
        let sig_bytes = sig_res?;

        let artifact = TetArtifact {
            manifest: serde_json::from_slice(&config_data)?,
            blueprint_wasm: wasm_bytes,
            vfs_zstd: vfs_bytes,
            signature: sig_bytes,
        };

        Ok(artifact)
    }

    async fn fetch_blob(&self, name: &str, digest: &str) -> Result<Vec<u8>> {
        let url = format!("{}/v2/{}/blobs/{}", self.registry_url, name, digest);
        let mut req = self.http.get(&url);
        if let Some(auth) = self.get_auth_header() {
            req = req.header("Authorization", auth);
        }
        
        let res = req.send().await?;
        if !res.status().is_success() {
            return Err(anyhow!("Failed to fetch blob {}: {}", digest, res.status()));
        }
        
        let data = res.bytes().await?.to_vec();
        
        // Verify digest
        let actual_digest = format!("sha256:{}", hex::encode(Sha256::digest(&data)));
        if actual_digest != digest {
            return Err(anyhow!("Digest mismatch: expected {}, got {}", digest, actual_digest));
        }
        
        Ok(data)
    }
}
