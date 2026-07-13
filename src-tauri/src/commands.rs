use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine;
use chrono::Local;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::api::{self, SharedState};
use crate::error::{Result, message};
use crate::stellaverse;

const PNG_PREFIX: &str = "data:image/png;base64,";
const MAX_SCREENSHOT_BYTES: usize = 10 * 1024 * 1024;
const MAX_DATA_TRANSFER_BYTES: u64 = 128 * 1024 * 1024;
const DATA_TRANSFER_FORMAT: &str = "l2tv-data-transfer";
const DATA_TRANSFER_VERSION: u64 = 1;
const DATA_TRANSFER_KEYS: [&str; 5] = [
    "form-state",
    "last-analysis",
    "table-preset-selection",
    "custom-table-list-entries",
    "stellaverse-rival-ids",
];

#[tauri::command]
pub async fn api_request(path: String, body: Value, state: SharedState<'_>) -> Result<Value> {
    api::request(&path, body, &state).await
}

#[tauri::command]
pub fn pick_file(options: Value) -> Result<String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(title) = options.get("title").and_then(Value::as_str) {
        dialog = dialog.set_title(title);
    }
    if let Some(path) = options.get("defaultPath").and_then(Value::as_str) {
        let path = PathBuf::from(path);
        let directory = if path.is_dir() {
            path
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };
        dialog = dialog.set_directory(directory);
    }
    if let Some(filters) = options.get("filters").and_then(Value::as_array) {
        for filter in filters {
            let name = filter.get("name").and_then(Value::as_str).unwrap_or("File");
            let extensions: Vec<&str> = filter
                .get("extensions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if !extensions.is_empty() {
                dialog = dialog.add_filter(name, &extensions);
            }
        }
    }
    Ok(dialog
        .pick_file()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default())
}

#[tauri::command]
pub fn pick_directory(options: Value) -> Result<String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(title) = options.get("title").and_then(Value::as_str) {
        dialog = dialog.set_title(title);
    }
    if let Some(path) = options.get("defaultPath").and_then(Value::as_str)
        && !path.trim().is_empty()
    {
        dialog = dialog.set_directory(path);
    }
    Ok(dialog
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default())
}

#[tauri::command]
pub fn save_image(app: AppHandle, options: Value) -> Result<Value> {
    let data_url = options
        .get("dataUrl")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let encoded = data_url
        .strip_prefix(PNG_PREFIX)
        .ok_or_else(|| message("PNGデータの保存に失敗しました。"))?;
    if encoded.len() > (MAX_SCREENSHOT_BYTES * 4 / 3) + 8 {
        return Err(message("画像データが大きすぎます。"));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| message("PNGデータの保存に失敗しました。"))?;
    if bytes.is_empty() || bytes.len() > MAX_SCREENSHOT_BYTES {
        return Err(message("画像データが空か、大きすぎます。"));
    }

    let requested_directory = options
        .get("directoryPath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let directory = if requested_directory.is_empty() {
        let preferred = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .map(|path| path.join("screenshot"));
        preferred
            .filter(|path| crate::external::ensure_writable_directory(path))
            .unwrap_or(
                app.path()
                    .app_data_dir()
                    .map_err(|error| message(error.to_string()))?
                    .join("screenshot"),
            )
    } else {
        PathBuf::from(requested_directory)
    };
    fs::create_dir_all(&directory)?;

    let requested_name = options
        .get("fileName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let file_name = sanitize_png_name(requested_name);
    let output = write_unique(&directory, &file_name, &bytes)?;
    Ok(json!({
        "filePath": output.to_string_lossy(),
        "directoryPath": directory.to_string_lossy(),
    }))
}

#[tauri::command]
pub fn export_data_transfer(options: Value) -> Result<Value> {
    let payload = options
        .get("payload")
        .ok_or_else(|| message("引継ぎデータの形式が正しくありません。"))?;
    validate_data_transfer(payload)?;
    let bytes = serde_json::to_vec_pretty(payload).map_err(|error| message(error.to_string()))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_DATA_TRANSFER_BYTES {
        return Err(message("引継ぎデータが空か、大きすぎます。"));
    }

    let requested_name = options
        .get("fileName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let file_name = sanitize_data_transfer_name(requested_name);
    let output = rfd::FileDialog::new()
        .set_title("L2TV 引継ぎデータを書き出す")
        .set_file_name(&file_name)
        .add_filter("L2TV data transfer", &["json"])
        .save_file();
    let Some(output) = output else {
        return Ok(Value::Null);
    };
    fs::write(&output, bytes)?;
    Ok(json!({ "filePath": output.to_string_lossy() }))
}

#[tauri::command]
pub fn import_data_transfer() -> Result<Value> {
    let source = rfd::FileDialog::new()
        .set_title("L2TV 引継ぎデータを読み込む")
        .add_filter("L2TV data transfer", &["json"])
        .pick_file();
    let Some(source) = source else {
        return Ok(Value::Null);
    };
    let metadata = fs::metadata(&source)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_DATA_TRANSFER_BYTES {
        return Err(message("引継ぎデータが空か、大きすぎます。"));
    }
    let bytes = fs::read(source)?;
    let payload: Value = serde_json::from_slice(&bytes)
        .map_err(|_| message("引継ぎデータの形式が正しくありません。"))?;
    validate_data_transfer(&payload)?;
    Ok(payload)
}

#[tauri::command]
pub async fn fetch_stellaverse_rival(options: Value, state: SharedState<'_>) -> Result<Value> {
    stellaverse::fetch_rival(options, &state).await
}

#[tauri::command]
pub async fn fetch_stellaverse_rankings(options: Value, state: SharedState<'_>) -> Result<Value> {
    stellaverse::fetch_rankings(options, &state).await
}

#[tauri::command]
pub fn write_e2e_marker(payload: Value, state: SharedState<'_>) -> Result<()> {
    let path = state
        .e2e_marker_path
        .as_ref()
        .ok_or_else(|| message("E2E marker is disabled."))?;
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|error| message(error.to_string()))?;
    if bytes.len() > 64 * 1024 {
        return Err(message("E2E marker payload is too large."));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn sanitize_png_name(value: &str) -> String {
    let fallback = format!("L2TV_Today_{}.png", Local::now().format("%Y%m%d%H%M%S"));
    let mut normalized: String = value
        .trim()
        .chars()
        .map(|ch| {
            if r#"\/:*?\"<>|"#.contains(ch) || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect();
    normalized = normalized.split_whitespace().collect::<Vec<_>>().join("_");
    if normalized.is_empty() {
        return fallback;
    }
    if !normalized.to_ascii_lowercase().ends_with(".png") {
        normalized.push_str(".png");
    }
    normalized.chars().take(180).collect()
}

fn sanitize_data_transfer_name(value: &str) -> String {
    let fallback = format!(
        "L2TV_data_transfer_{}.json",
        Local::now().format("%Y%m%d%H%M%S")
    );
    let mut normalized: String = value
        .trim()
        .chars()
        .map(|ch| {
            if r#"\/:*?\"<>|"#.contains(ch) || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect();
    normalized = normalized.split_whitespace().collect::<Vec<_>>().join("_");
    if normalized.is_empty() {
        return fallback;
    }
    if !normalized.to_ascii_lowercase().ends_with(".json") {
        normalized.push_str(".json");
    }
    normalized.chars().take(180).collect()
}

fn validate_data_transfer(payload: &Value) -> Result<()> {
    let Some(object) = payload.as_object() else {
        return Err(message("引継ぎデータの形式が正しくありません。"));
    };
    if object.get("format").and_then(Value::as_str) != Some(DATA_TRANSFER_FORMAT)
        || object.get("version").and_then(Value::as_u64) != Some(DATA_TRANSFER_VERSION)
    {
        return Err(message("引継ぎデータの形式が正しくありません。"));
    }
    let Some(data) = object.get("data").and_then(Value::as_object) else {
        return Err(message("引継ぎデータの形式が正しくありません。"));
    };
    if !DATA_TRANSFER_KEYS.iter().any(|key| data.contains_key(*key)) {
        return Err(message("引継ぎデータの形式が正しくありません。"));
    }
    Ok(())
}

fn write_unique(directory: &Path, file_name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("l2tv-image");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png");
    for index in 0..1000 {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("_{}", index + 1)
        };
        let candidate = directory.join(format!("{stem}{suffix}.{extension}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                file.write_all(bytes)?;
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(message("画像ファイル名を確保できませんでした。"))
}

#[cfg(test)]
mod tests {
    use super::{sanitize_data_transfer_name, validate_data_transfer};
    use serde_json::json;

    #[test]
    fn accepts_supported_data_transfer_payload() {
        let payload = json!({
            "format": "l2tv-data-transfer",
            "version": 1,
            "data": { "form-state": { "themeMode": "lr2ir-dark" } }
        });
        assert!(validate_data_transfer(&payload).is_ok());
    }

    #[test]
    fn rejects_unknown_or_empty_data_transfer_payloads() {
        assert!(validate_data_transfer(&json!({})).is_err());
        assert!(
            validate_data_transfer(&json!({
                "format": "l2tv-data-transfer",
                "version": 1,
                "data": { "unknown": true }
            }))
            .is_err()
        );
    }

    #[test]
    fn sanitizes_data_transfer_file_name() {
        assert_eq!(
            sanitize_data_transfer_name("L2TV: legacy data"),
            "L2TV__legacy_data.json"
        );
    }
}
