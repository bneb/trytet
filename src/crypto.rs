use ed25519_dalek::{Keypair, Signature, Signer, Verifier};
use rand::rngs::OsRng;
use std::fs;
use std::path::PathBuf;

pub struct AgentWallet {
    keypair: Keypair,
}

impl AgentWallet {
    pub fn load_or_create() -> anyhow::Result<Self> {
        let home_dir = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let trytet_dir = home_dir.join(".trytet");
        
        if !trytet_dir.exists() {
            fs::create_dir_all(&trytet_dir)?;
        }

        let key_path = trytet_dir.join("id_ed25519");

        let keypair = if key_path.exists() {
            let bytes = fs::read(&key_path)?;
            Keypair::from_bytes(&bytes)?
        } else {
            let mut csprng = OsRng{};
            let new_keypair = Keypair::generate(&mut csprng);
            fs::write(&key_path, new_keypair.to_bytes())?;
            new_keypair
        };

        Ok(Self { keypair })
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.keypair.public.as_bytes())
    }

    pub fn sign_manifest(&self, payload: &[u8]) -> String {
        let signature = self.keypair.sign(payload);
        hex::encode(signature.to_bytes())
    }

    pub fn verify_signature(pubkey_hex: &str, payload: &[u8], signature_hex: &str) -> bool {
        let pub_bytes = match hex::decode(pubkey_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig_bytes = match hex::decode(signature_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };

        let public_key = match ed25519_dalek::PublicKey::from_bytes(&pub_bytes) {
            Ok(pk) => pk,
            Err(_) => return false,
        };
        let signature = match Signature::from_bytes(&sig_bytes) {
            Ok(s) => s,
            Err(_) => return false,
        };

        public_key.verify(payload, &signature).is_ok()
    }
}
