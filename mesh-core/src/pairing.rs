use crate::DeviceIdentity;
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::Verifier;
use getrandom::fill;
use x25519_dalek::{PublicKey, StaticSecret};

#[derive(Debug, Clone)]
pub struct PairingRequest {
    pub device_id: String,
    pub device_name: String,
    pub identity_public_key: String,
    pub ephemeral_public_key: [u8; 32],
    pub challenge: [u8; 32],
    pub signature: String,
}

pub struct PairingSession {
    secret: StaticSecret,
    pub public_key: PublicKey,
}

impl PairingSession {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut secret_bytes = [0u8; 32];
        fill(&mut secret_bytes)?;

        let secret = StaticSecret::from(secret_bytes);
        let public_key = PublicKey::from(&secret);

        Ok(Self { secret, public_key })
    }

    pub fn create_request(
        &self,
        identity: &DeviceIdentity,
    ) -> Result<PairingRequest, Box<dyn std::error::Error>> {
        let mut challenge = [0u8; 32];
        fill(&mut challenge)?;

        let mut signed_data = Vec::new();
        signed_data.extend_from_slice(identity.id.as_bytes());
        signed_data.extend_from_slice(identity.name.as_bytes());
        signed_data.extend_from_slice(self.public_key.as_bytes());
        signed_data.extend_from_slice(&challenge);

        let signature = identity.sign(&signed_data)?;

        Ok(PairingRequest {
            device_id: identity.id.to_string(),
            device_name: identity.name.clone(),
            identity_public_key: identity.public_key.clone(),
            ephemeral_public_key: *self.public_key.as_bytes(),
            challenge,
            signature,
        })
    }

    pub fn verify_request(request: &PairingRequest) -> Result<bool, Box<dyn std::error::Error>> {
        let public_key = STANDARD.decode(&request.identity_public_key)?;
        let public_key: [u8; 32] = public_key
            .try_into()
            .map_err(|_| "Invalid identity public key")?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key)?;

        let device_id = uuid::Uuid::parse_str(&request.device_id)?;

        let mut signed_data = Vec::new();
        signed_data.extend_from_slice(device_id.as_bytes());
        signed_data.extend_from_slice(request.device_name.as_bytes());
        signed_data.extend_from_slice(&request.ephemeral_public_key);
        signed_data.extend_from_slice(&request.challenge);

        let signature_bytes = STANDARD.decode(&request.signature)?;
        let signature_bytes: [u8; 64] = signature_bytes
            .try_into()
            .map_err(|_| "Invalid signature")?;

        let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes);

        Ok(verifying_key.verify(&signed_data, &signature).is_ok())
    }

    pub fn shared_secret(&self, peer_public_key: &[u8; 32]) -> [u8; 32] {
        let peer = PublicKey::from(*peer_public_key);
        let shared = self.secret.diffie_hellman(&peer);

        *shared.as_bytes()
    }
}
