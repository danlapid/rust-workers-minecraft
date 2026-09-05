use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wasm_bindgen::prelude::*;
use worker::emscripten::{future_to_promise, js_error, socket_from_value};

fn main() {}

#[wasm_bindgen]
pub fn echo(socket: JsValue) -> js_sys::Promise {
    future_to_promise(async move {
        let mut socket = socket_from_value(socket)?;
        let mut bytes = [0u8; 16 * 1024];
        loop {
            let count = socket.read(&mut bytes).await.map_err(js_error)?;
            if count == 0 {
                break;
            }
            socket.write_all(&bytes[..count]).await.map_err(js_error)?;
        }
        socket.shutdown().await.map_err(js_error)?;
        Ok(JsValue::UNDEFINED)
    })
}

#[wasm_bindgen]
pub fn tick_after(milliseconds: u32) -> js_sys::Promise {
    future_to_promise(async move {
        tokio::time::sleep(std::time::Duration::from_millis(milliseconds.into())).await;
        Ok(JsValue::TRUE)
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
