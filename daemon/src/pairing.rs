use mesh_core::{
    DeviceIdentity, channel,
    pairing::{PairingRequest, PairingSession},
    secure::{SecureAck, SecureHello, derive_session_key},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Component, Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use workspace::{
    apply::{ApplyResult, apply_remote},
    conflict::FileVersion,
    state::StateStore,
};

const PAIRING_PORT: u16 = 45873;
const MAX_FRAME: usize = 64 * 1024;
const CHUNK_SIZE: usize = 32 * 1024;

#[derive(Debug, Serialize, Deserialize)]
struct FileStart {
    path: String,
    size: u64,
    sha256: String,
    version: u64,
    device_id: String,
    event_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileDelete {
    path: String,
}

async fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;

    let len = u32::from_be_bytes(len_buf) as usize;

    if len == 0 || len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid frame size").into());
    }

    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;
    Ok(data)
}

async fn write_frame(
    stream: &mut TcpStream,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    if data.is_empty() || data.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid frame size").into());
    }

    stream.write_all(&(data.len() as u32).to_be_bytes()).await?;
    stream.write_all(data).await?;
    Ok(())
}

fn safe_relative_path(path: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let candidate = Path::new(path);

    if candidate.is_absolute() {
        return Err("absolute paths are not allowed".into());
    }

    for component in candidate.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err("unsafe file path".into()),
        }
    }

    if path.is_empty() {
        return Err("empty file path".into());
    }

    Ok(candidate.to_path_buf())
}

async fn receive_file(
    stream: &mut TcpStream,
    key: &[u8; 32],
) -> Result<(), Box<dyn std::error::Error>> {
    let encrypted_start = read_frame(stream).await?;
    let start_plain = channel::decrypt(key, &encrypted_start)?;

    if start_plain.first() != Some(&1) {
        return Err("expected FILE_START".into());
    }

    if start_plain.len() < 5 {
        return Err("invalid FILE_START".into());
    }

    let meta_len = u32::from_be_bytes([
        start_plain[1],
        start_plain[2],
        start_plain[3],
        start_plain[4],
    ]) as usize;

    if meta_len > 32 * 1024 || start_plain.len() != 5 + meta_len {
        return Err("invalid metadata size".into());
    }

    let meta: FileStart = serde_json::from_slice(&start_plain[5..])?;

    let relative = safe_relative_path(&meta.path)?;

    let root = PathBuf::from("/var/lib/meshos/transfer");
    fs::create_dir_all(&root).await?;

    let destination = root.join(&relative);

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).await?;
    }

    let temp_name = format!(".{}.part", std::process::id());

    let temp = destination.parent().unwrap_or(&root).join(temp_name);

    let mut output = File::create(&temp).await?;
    let mut received: u64 = 0;
    let mut hasher = Sha256::new();

    loop {
        let encrypted = read_frame(stream).await?;
        let plain = channel::decrypt(key, &encrypted)?;

        if plain.is_empty() {
            return Err("empty transfer packet".into());
        }

        match plain[0] {
            2 => {
                if plain.len() < 9 {
                    return Err("invalid FILE_CHUNK".into());
                }

                let offset = u64::from_be_bytes([
                    plain[1], plain[2], plain[3], plain[4], plain[5], plain[6], plain[7], plain[8],
                ]);

                if offset != received {
                    return Err("unexpected file offset".into());
                }

                let chunk = &plain[9..];

                if chunk.is_empty() || chunk.len() > CHUNK_SIZE {
                    return Err("invalid chunk size".into());
                }

                output.write_all(chunk).await?;
                hasher.update(chunk);
                received += chunk.len() as u64;

                if received > meta.size {
                    return Err("received more data than declared".into());
                }
            }

            3 => {
                if plain.len() != 33 {
                    return Err("invalid FILE_FINISH".into());
                }

                if received != meta.size {
                    return Err("file size mismatch".into());
                }

                let actual = hasher.finalize();
                let actual_hex = actual
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();

                if actual_hex != meta.sha256 {
                    let _ = fs::remove_file(&temp).await;
                    return Err("SHA-256 verification failed".into());
                }

                output.flush().await?;
                drop(output);

                fs::rename(&temp, &destination).await?;

                println!("[MeshOS] File received successfully:");
                println!("  Path: {}", destination.display());
                println!("  Size: {} bytes", received);
                println!("  SHA-256: {}", actual_hex);

                let ack = channel::encrypt(key, b"FILE_OK")?;
                write_frame(stream, &ack).await?;

                return Ok(());
            }

            _ => return Err("unknown file packet".into()),
        }
    }
}

pub async fn run_pairing_server(
    identity: DeviceIdentity,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("0.0.0.0", PAIRING_PORT)).await?;

    println!("[MeshOS] Secure pairing listener: TCP {}", PAIRING_PORT);

    loop {
        let (mut stream, address) = listener.accept().await?;
        let identity = identity.clone();

        tokio::spawn(async move {
            match handle_connection(&mut stream, &identity).await {
                Ok(()) => {
                    println!("[MeshOS] Secure channel completed: {}", address);
                }
                Err(error) => {
                    eprintln!(
                        "[MeshOS] Secure connection rejected from {}: {}",
                        address, error
                    );
                }
            }
        });
    }
}

async fn handle_connection(
    stream: &mut TcpStream,
    identity: &DeviceIdentity,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = read_frame(stream).await?;

    let hello: SecureHello = serde_json::from_slice(&bytes)?;
    let request: PairingRequest = hello.into_pairing_request();

    if request.device_id == identity.id.to_string() {
        return Err("self-pairing rejected".into());
    }

    if !PairingSession::verify_request(&request)? {
        return Err("client identity verification failed".into());
    }

    let session = PairingSession::new()?;
    let session_key = derive_session_key(&session, &request.ephemeral_public_key)?;

    let ack = SecureAck::create(identity, &session, &request)?;
    write_frame(stream, &serde_json::to_vec(&ack)?).await?;

    println!("[MeshOS] Pair request verified");
    println!("  Remote device: {}", request.device_name);
    println!("  Remote ID: {}", request.device_id);
    println!("  Session key established: YES");

    let first = read_frame(stream).await?;
    let first_plain = channel::decrypt(&session_key, &first)?;

    match first_plain.first() {
        Some(0) => {
            println!(
                "[MeshOS] Encrypted message received: {}",
                String::from_utf8_lossy(&first_plain[1..])
            );

            let reply = channel::encrypt(&session_key, b"MeshOS secure channel confirmed")?;

            write_frame(stream, &reply).await?;
        }

        Some(4) => {
            if first_plain.len() < 5 {
                return Err("invalid FILE_DELETE".into());
            }

            let meta_len = u32::from_be_bytes([
                first_plain[1],
                first_plain[2],
                first_plain[3],
                first_plain[4],
            ]) as usize;

            if meta_len > 32 * 1024 || first_plain.len() != 5 + meta_len {
                return Err("invalid delete metadata".into());
            }

            let delete: FileDelete = serde_json::from_slice(&first_plain[5..])?;

            let relative = safe_relative_path(&delete.path)?;

            let root = PathBuf::from("/var/lib/meshos/transfer");

            let destination = root.join(relative);

            match fs::remove_file(&destination).await {
                Ok(()) => {
                    println!("[MeshOS] Remote delete: {}", destination.display());
                }

                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    println!(
                        "[MeshOS] Remote delete already absent: {}",
                        destination.display()
                    );
                }

                Err(error) => return Err(error.into()),
            }

            let ack = channel::encrypt(&session_key, b"DELETE_OK")?;

            write_frame(stream, &ack).await?;
        }

        Some(1) => {
            if first_plain.len() < 5 {
                return Err("invalid FILE_START".into());
            }

            let meta_len = u32::from_be_bytes([
                first_plain[1],
                first_plain[2],
                first_plain[3],
                first_plain[4],
            ]) as usize;

            if meta_len > 32 * 1024 || first_plain.len() != 5 + meta_len {
                return Err("invalid FILE_START".into());
            }

            let meta: FileStart = serde_json::from_slice(&first_plain[5..])?;

            let root = PathBuf::from("/var/lib/meshos/transfer");
            fs::create_dir_all(&root).await?;

            let relative = safe_relative_path(&meta.path)?;
            let destination = root.join(&relative);

            let mut received = Vec::new();
            let mut received_len = 0u64;
            let mut hasher = Sha256::new();

            loop {
                let encrypted = read_frame(stream).await?;
                let plain = channel::decrypt(&session_key, &encrypted)?;

                if plain.first() == Some(&2) {
                    if plain.len() < 9 {
                        return Err("invalid FILE_CHUNK".into());
                    }

                    let offset = u64::from_be_bytes([
                        plain[1], plain[2], plain[3], plain[4], plain[5], plain[6], plain[7],
                        plain[8],
                    ]);

                    if offset != received_len {
                        return Err("unexpected file offset".into());
                    }

                    let chunk = &plain[9..];

                    if chunk.is_empty() || chunk.len() > CHUNK_SIZE {
                        return Err("invalid chunk".into());
                    }

                    received.extend_from_slice(chunk);
                    hasher.update(chunk);
                    received_len += chunk.len() as u64;

                    if received_len > meta.size {
                        return Err("received more data than declared".into());
                    }
                } else if plain.first() == Some(&3) {
                    break;
                } else {
                    return Err("unexpected packet during file transfer".into());
                }
            }

            if received_len != meta.size {
                return Err("file size mismatch".into());
            }

            let actual_hash = hasher.finalize();
            let actual_hex = actual_hash
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();

            if actual_hex != meta.sha256 {
                return Err("SHA-256 verification failed".into());
            }

            let remote = FileVersion {
                path: meta.path.clone(),
                version: meta.version,
                sha256: meta.sha256.clone(),
                device_id: meta.device_id.clone(),
                event_id: meta.event_id.clone(),
            };

            let state_store = StateStore::new(&root)?;
            let state = state_store.load()?;
            let local = state.files.get(&meta.path).cloned();

            println!(
                "[MeshOS] Version check: local={:?}, remote={}",
                local.as_ref().map(|v| v.version),
                remote.version
            );

            match apply_remote(&root, local.as_ref(), &remote, &received)? {
                ApplyResult::AppliedRemote => {
                    println!("[MeshOS] Remote file applied: {}", destination.display());

                    state_store.upsert(state, remote.clone())?;
                }

                ApplyResult::AlreadyIdentical => {
                    println!("[MeshOS] File already identical: {}", meta.path);
                }

                ApplyResult::KeptLocal => {
                    println!("[MeshOS] Local version kept: {}", meta.path);
                }

                ApplyResult::ConflictCreated(path) => {
                    println!("[MeshOS] Conflict created: {}", path.display());
                }
            }

            let ack = channel::encrypt(&session_key, b"FILE_OK")?;

            write_frame(stream, &ack).await?;
        }

        _ => {
            return Err("unknown secure message type".into());
        }
    }

    Ok(())
}
