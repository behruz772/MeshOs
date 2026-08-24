use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::VerifyingKey;
use mesh_core::DeviceIdentity;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::sleep;

mod pairing;

const PORT: u16 = 45872;
const DISCOVER: &[u8] = b"MESHOS_DISCOVER_V1";
const DEVICE_PREFIX: &str = "MESHOS_DEVICE_V1|";

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TrustedDevice {
    id: String,
    name: String,
    address: String,
    public_key: String,
}

fn trusted_devices_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))?;
    Ok(PathBuf::from(home).join(".meshos_trusted_devices.json"))
}

fn load_trusted_devices() -> Result<Vec<TrustedDevice>, Box<dyn std::error::Error>> {
    let path = trusted_devices_path()?;

    if !path.exists() {
        return Ok(Vec::new());
    }

    let data = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

async fn run_discovery(identity: &DeviceIdentity) -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind(("0.0.0.0", PORT)).await?;

    if let Err(error) = socket.set_broadcast(true) {
        eprintln!("[MeshOS] Broadcast unavailable: {error}");
    }

    println!("[MeshOS] Discovery UDP {}", PORT);

    let mut buffer = [0u8; 2048];

    loop {
        match socket.send_to(DISCOVER, ("255.255.255.255", PORT)).await {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NetworkUnreachable => {
                println!("[MeshOS] Network unavailable; retrying...");
                sleep(Duration::from_secs(2)).await;
                continue;
            }
            Err(error) => return Err(error.into()),
        }

        match tokio::time::timeout(Duration::from_secs(3), socket.recv_from(&mut buffer)).await {
            Ok(Ok((size, addr))) => {
                handle_message(identity, &socket, &buffer[..size], addr).await?;
            }
            Ok(Err(error)) => {
                eprintln!("[MeshOS] Receive error: {error}");
            }
            Err(_) => {}
        }

        sleep(Duration::from_secs(1)).await;
    }
}

async fn handle_message(
    identity: &DeviceIdentity,
    socket: &UdpSocket,
    message: &[u8],
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    if message == DISCOVER {
        let response = format!(
            "{}{}|{}|{}",
            DEVICE_PREFIX, identity.id, identity.name, identity.public_key
        );

        socket.send_to(response.as_bytes(), addr).await?;
        println!("[MeshOS] Discovery request from {}", addr);
        return Ok(());
    }

    let Ok(message) = std::str::from_utf8(message) else {
        return Ok(());
    };

    let Some(data) = message.strip_prefix(DEVICE_PREFIX) else {
        return Ok(());
    };

    let parts: Vec<&str> = data.split('|').collect();

    if parts.len() != 3 {
        return Ok(());
    }

    let id = parts[0];
    let name = parts[1];
    let public_key = parts[2];

    // Never discover ourselves.
    if id == identity.id.to_string() {
        return Ok(());
    }

    let public_bytes = match STANDARD.decode(public_key) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(()),
    };

    let public_array: [u8; 32] = match public_bytes.try_into() {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };

    if VerifyingKey::from_bytes(&public_array).is_err() {
        return Ok(());
    }

    let trusted = load_trusted_devices()?;

    println!("[MeshOS] Found valid MeshOS device:");
    println!("  Name: {}", name);
    println!("  ID: {}", id);
    println!("  Address: {}", addr);

    if trusted.iter().any(|device| device.id == id) {
        println!("  Status: TRUSTED");
    } else {
        println!("  Status: PAIRING REQUIRED");
        println!("  Secure pairing must be approved before trust is granted.");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let identity = DeviceIdentity::load_or_create("Behruz-Laptop")?;

    println!("MeshOS Daemon");
    println!("Device: {}", identity.name);
    println!("ID: {}", identity.id);

    let trusted = load_trusted_devices()?;
    println!("Trusted devices: {}", trusted.len());

    let pairing_identity = identity.clone();

    tokio::spawn(async move {
        if let Err(error) = pairing::run_pairing_server(pairing_identity).await {
            eprintln!("[MeshOS] Secure pairing service stopped: {error}");
        }
    });

    run_discovery(&identity).await
}
