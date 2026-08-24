use std::net::SocketAddr;
use tokio::net::UdpSocket;

const DISCOVERY_PORT: u16 = 45872;
const DISCOVERY_MESSAGE: &[u8] = b"MESHOS_DISCOVER_V1";

pub async fn discover() -> Result<Vec<SocketAddr>, Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await?;
    socket.set_broadcast(true)?;

    socket
        .send_to(DISCOVERY_MESSAGE, ("255.255.255.255", DISCOVERY_PORT))
        .await?;

    let mut devices = Vec::new();
    let mut buffer = [0u8; 256];

    while let Ok((size, addr)) = socket.recv_from(&mut buffer).await {
        if &buffer[..size] == DISCOVERY_MESSAGE && !devices.contains(&addr) {
            devices.push(addr);
        }
    }

    Ok(devices)
}
