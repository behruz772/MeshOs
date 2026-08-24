use crate::DeviceIdentity;
use crate::pairing::{PairingRequest, PairingSession};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::Verifier;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const ACK_CONTEXT: &[u8] = b"MESHOS_PAIR_ACK_V1";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SecureHello {
    pub device_id: String,
    pub device_name: String,
    pub identity_public_key: String,
    pub ephemeral_public_key: [u8; 32],
    pub challenge: [u8; 32],
    pub signature: String,
}

impl From<PairingRequest> for SecureHello {
    fn from(value: PairingRequest) -> Self {
        Self {
            device_id: value.device_id,
            device_name: value.device_name,
            identity_public_key: value.identity_public_key,
            ephemeral_public_key: value.ephemeral_public_key,
            challenge: value.challenge,
            signature: value.signature,
        }
    }
}

impl SecureHello {
    pub fn into_pairing_request(self) -> PairingRequest {
        PairingRequest {
            device_id: self.device_id,
            device_name: self.device_name,
            identity_public_key: self.identity_public_key,
            ephemeral_public_key: self.ephemeral_public_key,
            challenge: self.challenge,
            signature: self.signature,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SecureAck {
    pub device_id: String,
    pub device_name: String,
    pub identity_public_key: String,
    pub ephemeral_public_key: [u8; 32],
    pub challenge: [u8; 32],
    pub signature: String,
}

impl SecureAck {
    pub fn create(
        identity: &DeviceIdentity,
        session: &PairingSession,
        request: &PairingRequest,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut signed = Vec::new();

        signed.extend_from_slice(ACK_CONTEXT);
        signed.extend_from_slice(identity.id.as_bytes());
        signed.extend_from_slice(request.device_id.as_bytes());
        signed.extend_from_slice(&request.ephemeral_public_key);
        signed.extend_from_slice(session.public_key.as_bytes());
        signed.extend_from_slice(&request.challenge);

        let signature = identity.sign(&signed)?;

        Ok(Self {
            device_id: identity.id.to_string(),
            device_name: identity.name.clone(),
            identity_public_key: identity.public_key.clone(),
            ephemeral_public_key: *session.public_key.as_bytes(),
            challenge: request.challenge,
            signature,
        })
    }

    pub fn verify(&self, request: &PairingRequest) -> Result<bool, Box<dyn std::error::Error>> {
        if self.challenge != request.challenge {
            return Ok(false);
        }

        let public_bytes = STANDARD.decode(&self.identity_public_key)?;
        let public_key: [u8; 32] = public_bytes
            .try_into()
            .map_err(|_| "invalid server public key")?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key)?;

        let server_id = uuid::Uuid::parse_str(&self.device_id)?;

        let mut signed = Vec::new();

        signed.extend_from_slice(ACK_CONTEXT);
        signed.extend_from_slice(server_id.as_bytes());
        signed.extend_from_slice(request.device_id.as_bytes());
        signed.extend_from_slice(&request.ephemeral_public_key);
        signed.extend_from_slice(&self.ephemeral_public_key);
        signed.extend_from_slice(&self.challenge);

        let signature_bytes = STANDARD.decode(&self.signature)?;
        let signature_bytes: [u8; 64] = signature_bytes
            .try_into()
            .map_err(|_| "invalid server signature")?;

        let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes);

        Ok(verifying_key.verify(&signed, &signature).is_ok())
    }
}

pub fn derive_session_key(
    session: &PairingSession,
    peer_public_key: &[u8; 32],
) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let shared = session.shared_secret(peer_public_key);

    let hk = Hkdf::<Sha256>::new(None, &shared);

    let mut key = [0u8; 32];
    hk.expand(b"MeshOS secure session v1", &mut key)?;

    Ok(key)
}
