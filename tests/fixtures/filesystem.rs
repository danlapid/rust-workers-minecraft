#![allow(deprecated)]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wasm_bindgen::prelude::*;

fn main() {}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}

/// Echo every accepted connection for the lifetime of the instance.
#[wasm_bindgen(jspi)]
pub fn echo_server(ready: js_sys::Function) -> Result<(), JsValue> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(js_error)?;
    runtime.block_on(async {
        let listener = TcpListener::bind("0.0.0.0:25565").await.map_err(js_error)?;
        ready.call0(&JsValue::UNDEFINED)?;
        loop {
            let (mut socket, _) = listener.accept().await.map_err(js_error)?;
            tokio::spawn(async move {
                let mut bytes = [0u8; 16 * 1024];
                loop {
                    match socket.read(&mut bytes).await {
                        Ok(0) | Err(_) => break,
                        Ok(count) => {
                            if socket.write_all(&bytes[..count]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                let _ = socket.shutdown().await;
            });
        }
    })
}

/// Exercise the filesystem syscall path before loading Pumpkin's world data.
#[wasm_bindgen]
pub fn filesystem_probe(value: String) -> Result<String, JsValue> {
    use std::io::{Read, Seek, SeekFrom, Write};
    std::fs::create_dir_all("transport-probe").map_err(js_error)?;
    let path = "transport-probe/state.bin";
    let previous = std::fs::read_to_string(path).unwrap_or_default();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open("transport-probe/pending.bin")
        .map_err(js_error)?;
    file.write_all(b"----").map_err(js_error)?;
    file.seek(SeekFrom::Start(0)).map_err(js_error)?;
    file.write_all(value.as_bytes()).map_err(js_error)?;
    file.set_len(value.len() as u64).map_err(js_error)?;
    file.sync_all().map_err(js_error)?;
    file.seek(SeekFrom::Start(0)).map_err(js_error)?;
    let mut check = String::new();
    file.read_to_string(&mut check).map_err(js_error)?;
    if check != value {
        return Err(js_error("Filesystem descriptor roundtrip failed"));
    }
    drop(file);
    std::fs::rename("transport-probe/pending.bin", path).map_err(js_error)?;
    let mut large = vec![0x5a; 3 * 1024 * 1024];
    std::fs::write("transport-probe/large.bin", &large).map_err(js_error)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open("transport-probe/large.bin")
        .map_err(js_error)?;
    file.seek(SeekFrom::Start(65534)).map_err(js_error)?;
    file.write_all(&[1, 2, 3, 4]).map_err(js_error)?;
    large[65534..65538].copy_from_slice(&[1, 2, 3, 4]);
    file.set_len(large.len() as u64 + 3).map_err(js_error)?;
    large.extend_from_slice(&[0, 0, 0]);
    file.sync_all().map_err(js_error)?;
    drop(file);
    if std::fs::read("transport-probe/large.bin").map_err(js_error)? != large {
        return Err(js_error("Large file roundtrip failed"));
    }
    Ok(previous)
}
