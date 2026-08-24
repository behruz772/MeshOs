use mesh_core::{
    DeviceIdentity, channel,
    pairing::PairingSession,
    secure::{SecureAck, SecureHello, derive_session_key},
};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use workspace::{
    conflict::FileVersion,
    protocol::{SyncAction, SyncRecord},
    queue::{PendingRecord, SyncQueue},
    state::StateStore,
};

const MAX_FRAME: usize = 64 * 1024;
const CHUNK_SIZE: usize = 32 * 1024;

fn meshos_data_root() -> PathBuf {
    if let Ok(path) = std::env::var("MESHOS_DATA_DIR") {
        return PathBuf::from(path);
    }

    PathBuf::from("/var/lib/meshos")
}

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

async fn write_frame(
    stream: &mut TcpStream,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    if data.is_empty() || data.len() > MAX_FRAME {
        return Err("invalid frame size".into());
    }

    stream.write_all(&(data.len() as u32).to_be_bytes()).await?;
    stream.write_all(data).await?;
    Ok(())
}

async fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;

    let len = u32::from_be_bytes(len_buf) as usize;

    if len == 0 || len > MAX_FRAME {
        return Err("invalid frame size".into());
    }

    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;

    Ok(data)
}

async fn connect_secure(
    address: &str,
) -> Result<(TcpStream, [u8; 32]), Box<dyn std::error::Error>> {
    let identity = DeviceIdentity::load_or_create("Behruz-Phone")?;

    let session = PairingSession::new()?;
    let request = session.create_request(&identity)?;
    let hello = SecureHello::from(request.clone());

    let mut stream = TcpStream::connect(address).await?;

    write_frame(&mut stream, &serde_json::to_vec(&hello)?).await?;

    let response = read_frame(&mut stream).await?;
    let ack: SecureAck = serde_json::from_slice(&response)?;

    if !ack.verify(&request)? {
        return Err("server identity verification failed".into());
    }

    let key = derive_session_key(&session, &ack.ephemeral_public_key)?;

    Ok((stream, key))
}

async fn send_file(
    address: &str,
    file_path: &Path,
    root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let relative = file_path
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/");

    let data = fs::read(file_path).await?;
    let size = data.len() as u64;
    let digest = Sha256::digest(&data);

    let sha256 = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    let identity = DeviceIdentity::load_or_create("Behruz-Phone")?;

    let data_root = meshos_data_root();
    std::fs::create_dir_all(&data_root)?;

    let state_store = StateStore::new(&data_root)?;
    let version = state_store.next_version()?;

    let metadata = FileStart {
        path: relative.clone(),
        size,
        sha256: sha256.clone(),
        version,
        device_id: identity.id.to_string(),
        event_id: uuid::Uuid::new_v4().to_string(),
    };

    let (mut stream, key) = connect_secure(address).await?;

    let metadata_json = serde_json::to_vec(&metadata)?;

    let mut start = Vec::with_capacity(5 + metadata_json.len());

    start.push(1);
    start.extend_from_slice(&(metadata_json.len() as u32).to_be_bytes());
    start.extend_from_slice(&metadata_json);

    write_frame(&mut stream, &channel::encrypt(&key, &start)?).await?;

    let mut offset = 0u64;

    for chunk in data.chunks(CHUNK_SIZE) {
        let mut packet = Vec::with_capacity(9 + chunk.len());

        packet.push(2);
        packet.extend_from_slice(&offset.to_be_bytes());
        packet.extend_from_slice(chunk);

        write_frame(&mut stream, &channel::encrypt(&key, &packet)?).await?;

        offset += chunk.len() as u64;
    }

    write_frame(&mut stream, &channel::encrypt(&key, &[3u8])?).await?;

    let ack = read_frame(&mut stream).await?;
    let ack = channel::decrypt(&key, &ack)?;

    if ack != b"FILE_OK" {
        return Err("receiver rejected file".into());
    }

    let version_record = FileVersion {
        path: relative.clone(),
        version,
        sha256: sha256.clone(),
        device_id: metadata.device_id.clone(),
        event_id: metadata.event_id.clone(),
    };

    state_store.upsert(state_store.load()?, version_record)?;

    println!("[MeshOS] Synced: {} ({} bytes)", relative, data.len());

    println!("[MeshOS] SHA-256: {}", sha256);

    Ok(())
}

async fn send_delete(address: &str, relative_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (mut stream, key) = connect_secure(address).await?;

    let delete = FileDelete {
        path: relative_path.to_string(),
    };

    let json = serde_json::to_vec(&delete)?;

    let mut packet = Vec::with_capacity(5 + json.len());

    packet.push(4);
    packet.extend_from_slice(&(json.len() as u32).to_be_bytes());
    packet.extend_from_slice(&json);

    write_frame(&mut stream, &channel::encrypt(&key, &packet)?).await?;

    let ack = read_frame(&mut stream).await?;
    let ack = channel::decrypt(&key, &ack)?;

    if ack != b"DELETE_OK" {
        return Err("receiver rejected delete".into());
    }

    println!("[MeshOS] Deleted remotely: {}", relative_path);

    Ok(())
}

fn build_sync_record(root: &Path, path: &Path, kind: &EventKind) -> Option<SyncRecord> {
    let relative = path
        .strip_prefix(root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");

    if relative.is_empty() || relative == ".meshos" || relative.starts_with(".meshos/") {
        return None;
    }

    let action = match kind {
        EventKind::Create(_) | EventKind::Modify(_) => SyncAction::Upsert,

        EventKind::Remove(_) => SyncAction::Delete,

        _ => return None,
    };

    match action {
        SyncAction::Upsert => {
            if !path.is_file() {
                return None;
            }

            let data = std::fs::read(path).ok()?;

            let digest = Sha256::digest(&data);

            let sha256 = digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();

            Some(SyncRecord {
                action,
                path: relative,
                size: data.len() as u64,
                sha256: Some(sha256),
                event_id: uuid::Uuid::new_v4().to_string(),
            })
        }

        SyncAction::Delete => Some(SyncRecord {
            action,
            path: relative,
            size: 0,
            sha256: None,
            event_id: uuid::Uuid::new_v4().to_string(),
        }),
    }
}

fn coalesce(records: Vec<PendingRecord>) -> Vec<PendingRecord> {
    let mut latest: HashMap<String, PendingRecord> = HashMap::new();

    for record in records {
        latest.insert(record.record.path.clone(), record);
    }

    latest.into_values().collect()
}

async fn process_queue(
    address: &str,
    root: &Path,
    queue: &SyncQueue,
) -> Result<(), Box<dyn std::error::Error>> {
    let pending = queue.load()?;

    if pending.is_empty() {
        return Ok(());
    }

    let records = coalesce(pending);

    let mut remaining = Vec::new();

    for mut item in records {
        let result = match item.record.action {
            SyncAction::Upsert => {
                let path = root.join(&item.record.path);

                if !path.is_file() {
                    Err("file disappeared before sync".into())
                } else {
                    send_file(address, &path, root).await
                }
            }

            SyncAction::Delete => send_delete(address, &item.record.path).await,
        };

        match result {
            Ok(()) => {
                println!("[MeshOS] Queue ACK: {}", item.record.path);
            }

            Err(error) => {
                item.attempts = item.attempts.saturating_add(1);
                item.last_error = Some(error.to_string());

                let backoff_shift = item.attempts.min(5);
                let delay_secs = (1u64 << backoff_shift).min(30);

                println!(
                    "[MeshOS] Offline/retry #{} in {}s: {}",
                    item.attempts, delay_secs, item.record.path
                );

                remaining.push(item);

                tokio::time::sleep(Duration::from_secs(delay_secs)).await;
            }
        }
    }

    queue.replace(&remaining)?;

    Ok(())
}

async fn sync_workspace(address: &str, root: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&root)?;

    let data_root = meshos_data_root();
    std::fs::create_dir_all(&data_root)?;

    let queue = SyncQueue::new(&data_root)?;

    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(
        move |result| {
            let _ = tx.send(result);
        },
        Config::default(),
    )?;

    watcher.watch(&root, RecursiveMode::Recursive)?;

    println!("[MeshOS] Offline-first sync started");
    println!("[MeshOS] Workspace: {}", root.display());
    println!("[MeshOS] Target: {}", address);
    println!("[MeshOS] Pending queue: {}", queue.load()?.len());

    loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(result) => {
                let event = result?;

                for path in event.paths {
                    if let Some(record) = build_sync_record(&root, &path, &event.kind) {
                        queue.push(record)?;

                        println!("[MeshOS] Queued: {}", path.display());
                    }
                }
            }

            Err(RecvTimeoutError::Timeout) => {}

            Err(RecvTimeoutError::Disconnected) => {
                return Err("workspace watcher disconnected".into());
            }
        }

        if let Err(error) = process_queue(address, &root, &queue).await {
            eprintln!("[MeshOS] Queue worker error: {}", error);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    match args.as_slice() {
        [_, command, address] if command == "pair" => {
            let (mut stream, key) = connect_secure(address).await?;

            let mut message = vec![0u8];
            message.extend_from_slice(b"MeshOS secure channel online");

            write_frame(&mut stream, &channel::encrypt(&key, &message)?).await?;

            let reply = read_frame(&mut stream).await?;

            let plain = channel::decrypt(&key, &reply)?;

            println!("Encrypted reply: {}", String::from_utf8(plain)?);

            println!("ENCRYPTED DATA CHANNEL: OK");
        }

        [_, command, address, file] if command == "send" => {
            let path = PathBuf::from(file);

            let root = path.parent().unwrap_or(Path::new("."));

            send_file(address, &path, root).await?;
        }

        [_, command, address, directory] if command == "sync" => {
            sync_workspace(address, PathBuf::from(directory)).await?;
        }

        _ => {
            println!("MeshOS CLI");
            println!();
            println!("  cli pair <ip:45873>");
            println!("  cli send <ip:45873> <file>");
            println!("  cli sync <ip:45873> <directory>");
        }
    }

    Ok(())
}
