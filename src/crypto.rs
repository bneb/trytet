use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::Rng;
use std::fs;
use std::path::PathBuf;

pub struct AgentWallet {
    signing_key: SigningKey,
}

impl AgentWallet {
    pub fn load_or_create() -> anyhow::Result<Self> {
        let home_dir = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let trytet_dir = home_dir.join(".trytet");

        if !trytet_dir.exists() {
            fs::create_dir_all(&trytet_dir)?;
        }

        let key_path = trytet_dir.join("id_ed25519");

        let signing_key = if key_path.exists() {
            let bytes = fs::read(&key_path)?;
            let array: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid key length"))?;
            SigningKey::from_bytes(&array)
        } else {
            let mut key_bytes = [0u8; 32];
            rand::rng().fill_bytes(&mut key_bytes);
            let new_key = SigningKey::from_bytes(&key_bytes);
            fs::write(&key_path, new_key.to_bytes())?;
            new_key
        };

        Ok(Self { signing_key })
    }

    pub fn public_key_hex(&self) -> String {
        let verifying_key = self.signing_key.verifying_key();
        hex::encode(verifying_key.to_bytes())
    }

    pub fn sign_manifest(&self, payload: &[u8]) -> String {
        let signature = self.signing_key.sign(payload);
        hex::encode(signature.to_bytes())
    }

    pub fn verify_signature(pubkey_hex: &str, payload: &[u8], signature_hex: &str) -> bool {
        let pub_bytes = match hex::decode(pubkey_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let pub_array: [u8; 32] = match pub_bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return false,
        };

        let sig_bytes = match hex::decode(signature_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig_array: [u8; 64] = match sig_bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => return false,
        };

        let verifying_key = match VerifyingKey::from_bytes(&pub_array) {
            Ok(pk) => pk,
            Err(_) => return false,
        };

        let signature = Signature::from_bytes(&sig_array);

        verifying_key.verify(payload, &signature).is_ok()
    }
}
