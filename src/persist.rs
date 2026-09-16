//! The world lives in the object's `/tmp` tree while it runs and in its SQLite
//! storage between runs: one row per file, written at checkpoint, read at start.

use crate::host::{js_error, sql};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use wasm_bindgen::JsValue;

pub const ROOT: &str = "/tmp/world";

pub fn prepare(storage: &JsValue) -> Result<(), JsValue> {
    sql(
        storage,
        "CREATE TABLE IF NOT EXISTS files (path TEXT PRIMARY KEY, data BLOB NOT NULL)",
        &[],
    )?;
    Ok(())
}

fn stored(storage: &JsValue) -> Result<HashMap<String, Vec<u8>>, JsValue> {
    let mut files = HashMap::new();
    for row in sql(storage, "SELECT path, data FROM files", &[])? {
        let [path, data] = row.as_slice() else {
            return Err(js_error("unexpected files row"));
        };
        let path = path.as_string().ok_or_else(|| js_error("path is not text"))?;
        let data = js_sys::Uint8Array::new(&data).to_vec();
        files.insert(path, data);
    }
    Ok(files)
}

/// Recreates the stored tree under `ROOT`.
pub fn restore(storage: &JsValue) -> Result<usize, JsValue> {
    let _ = fs::remove_dir_all(ROOT);
    fs::create_dir_all(ROOT).map_err(js_error)?;
    let files = stored(storage)?;
    for (path, data) in &files {
        let target = Path::new(ROOT).join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(js_error)?;
        }
        fs::write(target, data).map_err(js_error)?;
    }
    Ok(files.len())
}

/// Writes every file under `ROOT` that differs from storage and removes rows for
/// files that no longer exist. Returns (written, removed).
pub fn save(storage: &JsValue) -> Result<(usize, usize), JsValue> {
    let mut previous = stored(storage)?;
    let mut files = Vec::new();
    walk(Path::new(ROOT), &mut files).map_err(js_error)?;
    let mut written = 0;
    for file in files {
        let key = file
            .strip_prefix(ROOT)
            .map_err(js_error)?
            .to_string_lossy()
            .into_owned();
        let data = fs::read(&file).map_err(js_error)?;
        if previous.remove(&key).is_some_and(|old| old == data) {
            continue;
        }
        sql(
            storage,
            "INSERT OR REPLACE INTO files (path, data) VALUES (?, ?)",
            &[key.into(), js_sys::Uint8Array::from(data.as_slice()).into()],
        )?;
        written += 1;
    }
    let removed = previous.len();
    for key in previous.into_keys() {
        sql(storage, "DELETE FROM files WHERE path = ?", &[key.into()])?;
    }
    Ok((written, removed))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            walk(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}
