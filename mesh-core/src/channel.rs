use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};
use getrandom::fill;

const NONCE_SIZE: usize = 12;
const MAX_MESSAGE: usize = 1024 * 1024;

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if plaintext.len() > MAX_MESSAGE {
        return Err("message too large".into());
    }

    let cipher = ChaCha20Poly1305::new_from_slice(key)?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    fill(&mut nonce_bytes)?;

    let nonce = Nonce::try_from(&nonce_bytes[..])?;
    let ciphertext = cipher.encrypt(&nonce, plaintext)?;

    let mut packet = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    packet.extend_from_slice(&nonce_bytes);
    packet.extend_from_slice(&ciphertext);

    Ok(packet)
}

pub fn decrypt(key: &[u8; 32], packet: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if packet.len() < NONCE_SIZE {
        return Err("encrypted packet too small".into());
    }

    if packet.len() > NONCE_SIZE + MAX_MESSAGE + 16 {
        return Err("encrypted packet too large".into());
    }

    let cipher = ChaCha20Poly1305::new_from_slice(key)?;

    let nonce = Nonce::try_from(&packet[..NONCE_SIZE])?;
    let ciphertext = &packet[NONCE_SIZE..];

    Ok(cipher.decrypt(&nonce, ciphertext)?)
}
