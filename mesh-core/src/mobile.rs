use jni::objects::{JObject, JString};
use jni::JNIEnv;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use std::{
    error::Error,
    fs,
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use tokio::{
    fs::{self as tfs, File},
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{timeout, Duration},
};

use crate::{
    DeviceIdentity,
    channel,
    pairing::PairingSession,
    secure::{SecureAck, SecureHello, derive_session_key},
};

const DISCOVERY_PORT: u16 = 45872;
const PAIRING_PORT: u16 = 45873;
const DISCOVER: &[u8] = b"MESHOS_DISCOVER_V1";
const DEVICE_PREFIX: &str = "MESHOS_DEVICE_V1|";
const MAX_FRAME: usize = 64 * 1024;
const CHUNK_SIZE: usize = 32 * 1024;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MobileDevice {
    pub id: String,
    pub name: String,
    pub address: String,
    pub public_key: String,
    pub online: bool,
    pub trusted: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransferResult {
    pub ok: bool,
    pub file: String,
    pub bytes: u64,
    pub message: String,
}

fn json_error(message: impl Into<String>) -> String {
    serde_json::json!({
        "ok": false,
        "error": message.into()
    }).to_string()
}

fn make_string(env: JNIEnv, value: &str) -> jni::sys::jstring {
    match env.new_string(value) {
        Ok(v) => v.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn data_root() -> PathBuf {
    if let Ok(v) = std::env::var("MESHOS_DATA_DIR") {
        return PathBuf::from(v);
    }

    std::env::temp_dir().join("meshos-mobile")
}

fn trusted_file() -> PathBuf {
    data_root().join("trusted_devices.json")
}

fn transfers_file() -> PathBuf {
    data_root().join("transfers.json")
}

fn ensure_data() -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(data_root())?;
    Ok(())
}

fn read_trusted() -> Vec<MobileDevice> {
    let _ = ensure_data();

    fs::read_to_string(trusted_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_trusted(devices: &[MobileDevice]) -> Result<(), Box<dyn Error>> {
    ensure_data()?;
    fs::write(trusted_file(), serde_json::to_vec_pretty(devices)?)?;
    Ok(())
}

fn remember_trusted(mut device: MobileDevice) -> Result<(), Box<dyn Error>> {
    let mut all = read_trusted();

    all.retain(|x| x.id != device.id && x.address != device.address);

    device.trusted = true;
    all.push(device);

    write_trusted(&all)
}

async fn read_frame(
    stream: &mut TcpStream,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;

    let len = u32::from_be_bytes(len_buf) as usize;

    if len == 0 || len > MAX_FRAME {
        return Err(
            io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid frame size",
            ).into()
        );
    }

    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;

    Ok(data)
}

async fn write_frame(
    stream: &mut TcpStream,
    data: &[u8],
) -> Result<(), Box<dyn Error>> {
    if data.is_empty() || data.len() > MAX_FRAME {
        return Err(
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid frame size",
            ).into()
        );
    }

    stream.write_all(&(data.len() as u32).to_be_bytes()).await?;
    stream.write_all(data).await?;

    Ok(())
}

async fn establish_session(
    address: &str,
    device_name: &str,
) -> Result<(TcpStream, [u8; 32], DeviceIdentity, SecureAck), Box<dyn Error>> {
    let identity = DeviceIdentity::load_or_create(device_name)?;

    let session = PairingSession::new()?;
    let request = session.create_request(&identity)?;
    let hello: SecureHello = request.clone().into();

    let target: SocketAddr =
        format!("{}:{}", address, PAIRING_PORT).parse()?;

    let mut stream = TcpStream::connect(target).await?;

    write_frame(
        &mut stream,
        &serde_json::to_vec(&hello)?
    ).await?;

    let ack: SecureAck =
        serde_json::from_slice(
            &read_frame(&mut stream).await?
        )?;

    if !ack.verify(&request)? {
        return Err("server identity verification failed".into());
    }

    let session_key =
        derive_session_key(
            &session,
            &ack.ephemeral_public_key
        )?;

    Ok((stream, session_key, identity, ack))
}

async fn secure_confirm(
    stream: &mut TcpStream,
    key: &[u8; 32],
) -> Result<(), Box<dyn Error>> {
    let hello = channel::encrypt(
        key,
        b"\x00MeshOS secure channel",
    )?;

    write_frame(stream, &hello).await?;

    let reply =
        channel::decrypt(
            key,
            &read_frame(stream).await?,
        )?;

    if reply.as_slice() != b"MeshOS secure channel confirmed" {
        return Err("secure channel confirmation failed".into());
    }

    Ok(())
}

async fn pair(
    address: &str,
    device_name: &str,
) -> Result<String, Box<dyn Error>> {
    let (mut stream, key, _identity, ack) =
        establish_session(address, device_name).await?;

    secure_confirm(&mut stream, &key).await?;

    let device = MobileDevice {
        id: ack.device_id.clone(),
        name: ack.device_name.clone(),
        address: address.to_string(),
        public_key: ack.identity_public_key.clone(),
        online: true,
        trusted: true,
    };

    remember_trusted(device.clone())?;

    Ok(serde_json::to_string(&serde_json::json!({
        "ok": true,
        "device_id": device.id,
        "device_name": device.name,
        "address": device.address,
        "trusted": true,
        "message": "Secure pairing completed and persisted"
    }))?)
}

async fn discover(
    own_id: &str,
) -> Result<Vec<MobileDevice>, Box<dyn Error>> {
    ensure_data()?;

    let socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
    socket.set_broadcast(true)?;

    socket
        .send_to(
            DISCOVER,
            ("255.255.255.255", DISCOVERY_PORT)
        )
        .await?;

    let trusted = read_trusted();

    let mut result = Vec::new();
    let mut buffer = [0u8; 2048];

    loop {
        match timeout(
            Duration::from_millis(1200),
            socket.recv_from(&mut buffer)
        ).await {
            Ok(Ok((size, addr))) => {
                let Ok(text) =
                    std::str::from_utf8(&buffer[..size])
                else {
                    continue;
                };

                let Some(data) =
                    text.strip_prefix(DEVICE_PREFIX)
                else {
                    continue;
                };

                let parts: Vec<&str> =
                    data.split('|').collect();

                if parts.len() != 3 {
                    continue;
                }

                if parts[0] == own_id {
                    continue;
                }

                let trusted_flag =
                    trusted.iter().any(|d| {
                        d.id == parts[0] ||
                        d.address == addr.ip().to_string()
                    });

                result.push(MobileDevice {
                    id: parts[0].to_string(),
                    name: parts[1].to_string(),
                    address: addr.ip().to_string(),
                    public_key: parts[2].to_string(),
                    online: true,
                    trusted: trusted_flag,
                });
            }

            Ok(Err(e)) => return Err(e.into()),
            Err(_) => break,
        }
    }

    result.sort_by(|a,b| a.name.cmp(&b.name));
    result.dedup_by(|a,b| a.id == b.id);

    Ok(result)
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 1024];

    loop {
        let n = std::io::Read::read(&mut file, &mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    )
}

async fn send_file(
    address: &str,
    device_name: &str,
    local_path: &str,
    remote_path: &str,
) -> Result<TransferResult, Box<dyn Error>> {
    let path = PathBuf::from(local_path);
    let meta = tfs::metadata(&path).await?;

    let size = meta.len();
    let checksum = sha256_file(&path)?;

    let (mut stream, key, identity, _ack) =
        establish_session(address, device_name).await?;

    let event_id =
        uuid::Uuid::new_v4().to_string();

    #[derive(Serialize)]
    struct FileStart {
        path: String,
        size: u64,
        sha256: String,
        version: u64,
        device_id: String,
        event_id: String,
    }

    let start = FileStart {
        path: remote_path.to_string(),
        size,
        sha256: checksum,
        version: 1,
        device_id: identity.id.to_string(),
        event_id,
    };

    let meta_bytes =
        serde_json::to_vec(&start)?;

    let mut packet =
        Vec::with_capacity(5 + meta_bytes.len());

    packet.push(1);
    packet.extend_from_slice(
        &(meta_bytes.len() as u32).to_be_bytes()
    );
    packet.extend_from_slice(&meta_bytes);

    write_frame(
        &mut stream,
        &channel::encrypt(&key, &packet)?,
    ).await?;

    let mut file =
        File::open(&path).await?;

    let mut buffer =
        vec![0u8; CHUNK_SIZE];

    let mut offset = 0u64;

    loop {
        let n =
            file.read(&mut buffer).await?;

        if n == 0 {
            break;
        }

        let mut chunk =
            Vec::with_capacity(9 + n);

        chunk.push(2);
        chunk.extend_from_slice(
            &offset.to_be_bytes()
        );
        chunk.extend_from_slice(&buffer[..n]);

        write_frame(
            &mut stream,
            &channel::encrypt(&key, &chunk)?,
        ).await?;

        offset += n as u64;
    }

    let finish =
        vec![3u8; 33];

    write_frame(
        &mut stream,
        &channel::encrypt(&key, &finish)?,
    ).await?;

    let ack =
        channel::decrypt(
            &key,
            &read_frame(&mut stream).await?
        )?;

    if ack.as_slice() != b"FILE_OK" {
        return Err(
            "remote device did not acknowledge file".into()
        );
    }

    let result =
        TransferResult {
            ok: true,
            file: remote_path.to_string(),
            bytes: size,
            message: "Transfer completed".to_string(),
        };

    ensure_data()?;

    let history =
        fs::read_to_string(transfers_file())
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<TransferResult>>(&s).ok())
            .unwrap_or_default();

    let mut history = history;
    history.push(result.clone());

    fs::write(
        transfers_file(),
        serde_json::to_vec_pretty(&history)?
    )?;

    Ok(result)
}

fn start_receiver(
    device_name: String,
) {
    std::thread::spawn(move || {
        let runtime =
            match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(_) => return,
            };

        runtime.block_on(async move {
            let identity =
                match DeviceIdentity::load_or_create(&device_name) {
                    Ok(v) => v,
                    Err(_) => return,
                };

            let listener =
                match TcpListener::bind(
                    ("0.0.0.0", PAIRING_PORT)
                ).await {
                    Ok(v) => v,
                    Err(_) => return,
                };

            loop {
                let Ok((mut stream, _addr)) =
                    listener.accept().await
                else {
                    continue;
                };

                let identity = identity.clone();

                tokio::spawn(async move {
                    let _ =
                        receive_connection(
                            &mut stream,
                            &identity
                        ).await;
                });
            }
        });
    });
}

async fn receive_connection(
    stream: &mut TcpStream,
    identity: &DeviceIdentity,
) -> Result<(), Box<dyn Error>> {
    let hello =
        serde_json::from_slice::<SecureHello>(
            &read_frame(stream).await?
        )?;

    let request =
        hello.into_pairing_request();

    if request.device_id ==
       identity.id.to_string() {
        return Err("self pairing rejected".into());
    }

    if !PairingSession::verify_request(&request)? {
        return Err("invalid remote identity".into());
    }

    let session =
        PairingSession::new()?;

    let key =
        derive_session_key(
            &session,
            &request.ephemeral_public_key
        )?;

    let ack =
        SecureAck::create(
            identity,
            &session,
            &request
        )?;

    write_frame(
        stream,
        &serde_json::to_vec(&ack)?
    ).await?;

    let first =
        channel::decrypt(
            &key,
            &read_frame(stream).await?
        )?;

    match first.first() {
        Some(0) => {
            let reply =
                channel::encrypt(
                    &key,
                    b"MeshOS secure channel confirmed"
                )?;

            write_frame(
                stream,
                &reply
            ).await?;
        }

        Some(1) => {
            receive_file(
                stream,
                &key
            ).await?;
        }

        _ => {}
    }

    Ok(())
}

async fn receive_file(
    stream: &mut TcpStream,
    key: &[u8; 32],
) -> Result<(), Box<dyn Error>> {
    let encrypted =
        read_frame(stream).await?;

    let start =
        channel::decrypt(key, &encrypted)?;

    if start.first() != Some(&1) {
        return Err("expected FILE_START".into());
    }

    let meta_len =
        u32::from_be_bytes([
            start[1],
            start[2],
            start[3],
            start[4],
        ]) as usize;

    if meta_len > 32 * 1024 ||
       start.len() != 5 + meta_len {
        return Err("invalid FILE_START".into());
    }

    #[derive(Deserialize)]
    struct FileStart {
        path: String,
        size: u64,
        sha256: String,
        version: u64,
        device_id: String,
        event_id: String,
    }

    let meta: FileStart =
        serde_json::from_slice(
            &start[5..]
        )?;

    let safe =
        Path::new(&meta.path);

    if safe.is_absolute() ||
       safe.components().any(|c| {
           !matches!(
               c,
               std::path::Component::Normal(_)
           )
       }) {
        return Err("unsafe remote path".into());
    }

    let root =
        data_root().join("received");

    tfs::create_dir_all(&root).await?;

    let dest =
        root.join(&meta.path);

    if let Some(parent) =
        dest.parent() {
        tfs::create_dir_all(parent).await?;
    }

    let part =
        dest.with_extension("meshos.part");

    let mut output =
        File::create(&part).await?;

    let mut hasher =
        Sha256::new();

    let mut received = 0u64;

    loop {
        let packet =
            channel::decrypt(
                key,
                &read_frame(stream).await?
            )?;

        match packet.first() {
            Some(2) => {
                if packet.len() < 9 {
                    return Err("invalid chunk".into());
                }

                let offset =
                    u64::from_be_bytes([
                        packet[1],
                        packet[2],
                        packet[3],
                        packet[4],
                        packet[5],
                        packet[6],
                        packet[7],
                        packet[8],
                    ]);

                if offset != received {
                    return Err(
                        "unexpected file offset".into()
                    );
                }

                let chunk =
                    &packet[9..];

                if chunk.is_empty() ||
                   chunk.len() > CHUNK_SIZE {
                    return Err(
                        "invalid chunk size".into()
                    );
                }

                output.write_all(chunk).await?;
                hasher.update(chunk);

                received +=
                    chunk.len() as u64;
            }

            Some(3) => {
                if received != meta.size {
                    return Err(
                        "file size mismatch".into()
                    );
                }

                let actual =
                    hasher
                        .finalize()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();

                if actual != meta.sha256 {
                    let _ =
                        tfs::remove_file(&part).await;

                    return Err(
                        "sha256 verification failed".into()
                    );
                }

                output.flush().await?;
                drop(output);

                if dest.exists() {
                    let backup =
                        dest.with_extension(
                            format!(
                                "conflict-{}",
                                uuid::Uuid::new_v4()
                            )
                        );

                    tfs::rename(
                        &dest,
                        backup
                    ).await?;
                }

                tfs::rename(
                    &part,
                    &dest
                ).await?;

                let encrypted_ok =
                    channel::encrypt(
                        key,
                        b"FILE_OK"
                    )?;

                write_frame(
                    stream,
                    &encrypted_ok
                ).await?;

                let _ =
                    meta.version;

                let _ =
                    meta.device_id;

                let _ =
                    meta.event_id;

                return Ok(());
            }

            _ => {
                return Err(
                    "unknown transfer packet".into()
                );
            }
        }
    }
}

fn start_receiver_once(
    device_name: &str
) {
    static STARTED:
        std::sync::Once =
        std::sync::Once::new();

    let name =
        device_name.to_string();

    STARTED.call_once(move || {
        start_receiver(name);
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_meshos_mobile_MeshNative_version(
    env: JNIEnv,
    _obj: JObject
) -> jni::sys::jstring {
    make_string(
        env,
        "MeshOS Mobile Full Backend 0.3"
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_meshos_mobile_MeshNative_ping(
    env: JNIEnv,
    _obj: JObject
) -> jni::sys::jstring {
    make_string(
        env,
        "MeshOS native backend: OK"
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_meshos_mobile_MeshNative_startReceiver(
    mut env: JNIEnv,
    _obj: JObject,
    device_name: JString
) -> jni::sys::jstring {
    let name: String =
        match env.get_string(&device_name) {
            Ok(v) => v.into(),
            Err(_) => {
                return make_string(
                    env,
                    &json_error(
                        "invalid device name"
                    )
                );
            }
        };

    start_receiver_once(&name);

    make_string(
        env,
        r#"{"ok":true,"message":"receiver_started"}"#
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_meshos_mobile_MeshNative_discover(
    mut env: JNIEnv,
    _obj: JObject,
    own_id: JString
) -> jni::sys::jstring {
    let own_id: String =
        match env.get_string(&own_id) {
            Ok(v) => v.into(),
            Err(_) => {
                return make_string(
                    env,
                    &json_error(
                        "invalid own_id"
                    )
                );
            }
        };

    let runtime =
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build() {
            Ok(v) => v,
            Err(e) => {
                return make_string(
                    env,
                    &json_error(e.to_string())
                );
            }
        };

    match runtime.block_on(
        discover(&own_id)
    ) {
        Ok(devices) =>
            make_string(
                env,
                &serde_json::to_string(
                    &devices
                ).unwrap_or_else(
                    |e| json_error(e.to_string())
                )
            ),

        Err(e) =>
            make_string(
                env,
                &json_error(e.to_string())
            ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_meshos_mobile_MeshNative_pair(
    mut env: JNIEnv,
    _obj: JObject,
    address: JString,
    device_name: JString
) -> jni::sys::jstring {
    let address: String =
        match env.get_string(&address) {
            Ok(v) => v.into(),
            Err(_) =>
                return make_string(
                    env,
                    &json_error(
                        "invalid address"
                    )
                ),
        };

    let name: String =
        match env.get_string(&device_name) {
            Ok(v) => v.into(),
            Err(_) =>
                return make_string(
                    env,
                    &json_error(
                        "invalid device name"
                    )
                ),
        };

    start_receiver_once(&name);

    let runtime =
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build() {
            Ok(v) => v,
            Err(e) =>
                return make_string(
                    env,
                    &json_error(e.to_string())
                ),
        };

    match runtime.block_on(
        pair(&address, &name)
    ) {
        Ok(v) =>
            make_string(env, &v),

        Err(e) =>
            make_string(
                env,
                &json_error(e.to_string())
            ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_meshos_mobile_MeshNative_sendFile(
    mut env: JNIEnv,
    _obj: JObject,
    address: JString,
    device_name: JString,
    local_path: JString,
    remote_path: JString
) -> jni::sys::jstring {
    let address: String =
        match env.get_string(&address) {
            Ok(v) => v.into(),
            Err(_) =>
                return make_string(
                    env,
                    &json_error("invalid address")
                ),
        };

    let name: String =
        match env.get_string(&device_name) {
            Ok(v) => v.into(),
            Err(_) =>
                return make_string(
                    env,
                    &json_error("invalid device name")
                ),
        };

    let local_path: String =
        match env.get_string(&local_path) {
            Ok(v) => v.into(),
            Err(_) =>
                return make_string(
                    env,
                    &json_error("invalid local path")
                ),
        };

    let remote_path: String =
        match env.get_string(&remote_path) {
            Ok(v) => v.into(),
            Err(_) =>
                return make_string(
                    env,
                    &json_error("invalid remote path")
                ),
        };

    start_receiver_once(&name);

    let runtime =
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build() {
            Ok(v) => v,
            Err(e) =>
                return make_string(
                    env,
                    &json_error(e.to_string())
                ),
        };

    match runtime.block_on(
        send_file(
            &address,
            &name,
            &local_path,
            &remote_path
        )
    ) {
        Ok(v) =>
            make_string(
                env,
                &serde_json::to_string(&v)
                    .unwrap_or_else(
                        |e| json_error(e.to_string())
                    )
            ),

        Err(e) =>
            make_string(
                env,
                &json_error(e.to_string())
            ),
    }
}
