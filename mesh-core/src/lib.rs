use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

const IDENTITY_FILE: &str = ".meshos_identity.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub id: Uuid,
    pub name: String,
    pub public_key: String,
    private_key: String,
}

impl DeviceIdentity {
    pub fn load_or_create(name: impl Into<String>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::identity_path()?;

        if path.exists() {
            let data = fs::read_to_string(&path)?;
            return Ok(serde_json::from_str(&data)?);
        }

        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret)?;

        let signing_key = SigningKey::from_bytes(&secret);
        let verifying_key = signing_key.verifying_key();

        let identity = Self {
            id: Uuid::new_v4(),
            name: name.into(),
            public_key: STANDARD.encode(verifying_key.to_bytes()),
            private_key: STANDARD.encode(signing_key.to_bytes()),
        };

        let data = serde_json::to_string_pretty(&identity)?;
        fs::write(&path, data)?;

        Ok(identity)
    }

    pub fn sign(&self, message: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
        let private_bytes = STANDARD.decode(&self.private_key)?;
        let private_array: [u8; 32] = private_bytes
            .try_into()
            .map_err(|_| "Invalid private key")?;

        let signing_key = SigningKey::from_bytes(&private_array);
        let signature = signing_key.sign(message);

        Ok(STANDARD.encode(signature.to_bytes()))
    }

    pub fn verify(
        public_key: &str,
        message: &[u8],
        signature: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let public_bytes = STANDARD.decode(public_key)?;
        let public_array: [u8; 32] = public_bytes.try_into().map_err(|_| "Invalid public key")?;

        let verifying_key = VerifyingKey::from_bytes(&public_array)?;

        let signature_bytes = STANDARD.decode(signature)?;
        let signature_array: [u8; 64] = signature_bytes
            .try_into()
            .map_err(|_| "Invalid signature")?;

        let signature = Signature::from_bytes(&signature_array);

        Ok(verifying_key.verify(message, &signature).is_ok())
    }

    fn identity_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))?;
        Ok(PathBuf::from(home).join(IDENTITY_FILE))
    }
}

pub mod discovery;

pub mod pairing;

pub mod secure;

pub mod channel;

#[cfg(test)]
mod channel_tests {
    use super::channel;

    #[test]
    fn encrypted_message_round_trip() {
        let key = [7u8; 32];
        let message = b"MeshOS secure channel online";

        let encrypted = channel::encrypt(&key, message).expect("encryption failed");

        assert_ne!(encrypted, message);

        let decrypted = channel::decrypt(&key, &encrypted).expect("decryption failed");

        assert_eq!(decrypted, message);
    }

    #[test]
    fn tampered_message_is_rejected() {
        let key = [9u8; 32];
        let message = b"MeshOS integrity test";

        let mut encrypted = channel::encrypt(&key, message).expect("encryption failed");

        let last = encrypted.len() - 1;
        encrypted[last] ^= 0x01;

        assert!(channel::decrypt(&key, &encrypted).is_err());
    }
}
