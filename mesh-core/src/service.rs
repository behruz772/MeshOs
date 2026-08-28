use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

const PORT: u16 = 45872;
const DISCOVER: &[u8] = b"MESHOS_DISCOVER_V1";
const DEVICE_PREFIX: &str = "MESHOS_DEVICE_V1|";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub address: String,
    pub public_key: String,
    pub trusted: bool,
    pub online: bool,
}

pub async fn discover_devices(
    own_id: &str,
) -> Result<Vec<DeviceInfo>, Box<dyn Error>> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
    socket.set_broadcast(true)?;

    socket.send_to(DISCOVER, ("255.255.255.255", PORT)).await?;

    let mut found = Vec::new();
    let mut buffer = [0u8; 2048];

    loop {
        match timeout(
            Duration::from_millis(1200),
            socket.recv_from(&mut buffer),
        )
        .await
        {
            Ok(Ok((size, addr))) => {
                if let Some(device) =
                    parse_device(&buffer[..size], own_id, addr)?
                {
                    if !found.iter().any(|d: &DeviceInfo| d.id == device.id) {
                        found.push(device);
                    }
                }
            }

            Ok(Err(error)) => return Err(error.into()),

            Err(_) => break,
        }
    }

    Ok(found)
}

fn parse_device(
    message: &[u8],
    own_id: &str,
    addr: SocketAddr,
) -> Result<Option<DeviceInfo>, Box<dyn Error>> {
    let Ok(message) = std::str::from_utf8(message) else {
        return Ok(None);
    };

    let Some(data) = message.strip_prefix(DEVICE_PREFIX) else {
        return Ok(None);
    };

    let parts: Vec<&str> = data.split('|').collect();

    if parts.len() != 3 {
        return Ok(None);
    }

    let id = parts[0];

    if id == own_id {
        return Ok(None);
    }

    let name = parts[1].to_string();
    let public_key = parts[2].to_string();

    let public_bytes = STANDARD.decode(&public_key)?;

    let public_array: [u8; 32] = public_bytes
        .try_into()
        .map_err(|_| "invalid public key length")?;

    VerifyingKey::from_bytes(&public_array)
        .map_err(|_| "invalid public key")?;

    Ok(Some(DeviceInfo {
        id: id.to_string(),
        name,
        address: addr.ip().to_string(),
        public_key,
        trusted: false,
        online: true,
    }))
}
