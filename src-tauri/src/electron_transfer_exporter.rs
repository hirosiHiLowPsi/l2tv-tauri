use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use url::Url;
use walkdir::WalkDir;

const PORTABLE_DATA_DIR: &str = "lr2ir-table-lamp-viewer-data";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 4173;
const MAX_EXPORT_BYTES: usize = 128 * 1024 * 1024;
const MAX_PROFILE_COPY_BYTES: u64 = 512 * 1024 * 1024;
const DATA_TRANSFER_KEYS: [&str; 5] = [
    "form-state",
    "last-analysis",
    "table-preset-selection",
    "custom-table-list-entries",
    "stellaverse-rival-ids",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacyOrigin {
    host: String,
    port: u16,
    stored_bytes: u64,
}

struct ExportPlan {
    output_path: PathBuf,
    profile_directory: PathBuf,
    origin: LegacyOrigin,
    listener: TcpListener,
    auto_exit: bool,
}

struct ExportServerState {
    output_path: PathBuf,
    auto_exit: bool,
    app: tauri::AppHandle,
}

pub fn should_run() -> bool {
    std::env::args_os().any(|argument| argument == "--electron-data-exporter")
        || std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_stem()
                    .map(|value| value.to_string_lossy().to_lowercase())
            })
            .is_some_and(|name| name.contains("electron-data-exporter"))
}

pub fn run() {
    let plan = match prepare_export() {
        Ok(Some(plan)) => plan,
        Ok(None) => return,
        Err(error) => {
            show_error(&error);
            return;
        }
    };

    let profile_directory = plan.profile_directory.clone();
    let origin = format!("http://{}:{}", plan.origin.host, plan.origin.port);
    let page_url = match Url::parse(&format!("{origin}/")) {
        Ok(url) => url,
        Err(error) => {
            show_error(&format!(
                "旧データの保存元URLを解釈できませんでした。\n{error}"
            ));
            cleanup_profile(&profile_directory);
            return;
        }
    };
    let allowed_origin = origin.clone();
    let output_path = plan.output_path;
    let listener = plan.listener;
    let auto_exit = plan.auto_exit;
    let webview_profile_directory = profile_directory.clone();

    let result = tauri::Builder::default()
        .setup(move |app| {
            let server_state = Arc::new(ExportServerState {
                output_path: output_path.clone(),
                auto_exit,
                app: app.handle().clone(),
            });
            start_server(listener, server_state);

            let navigation_origin = allowed_origin.clone();
            let window = tauri::WebviewWindowBuilder::new(
                app,
                "electron-data-exporter",
                tauri::WebviewUrl::External(page_url.clone()),
            )
            .title("L2TV Electron版データ引継ぎツール")
            .inner_size(720.0, 520.0)
            .min_inner_size(640.0, 460.0)
            .resizable(true)
            .center()
            .disable_drag_drop_handler()
            .data_directory(webview_profile_directory.clone())
            .on_navigation(move |url| url.origin().ascii_serialization() == navigation_origin)
            .on_new_window(|_url, _features| tauri::webview::NewWindowResponse::Deny)
            .build()?;
            window.show()?;
            Ok(())
        })
        .run(tauri::generate_context!());

    cleanup_profile(&profile_directory);
    if let Err(error) = result {
        show_error(&format!("引継ぎツールを起動できませんでした。\n{error}"));
    }
}

fn prepare_export() -> Result<Option<ExportPlan>, String> {
    if std::env::var_os("L2TV_EXPORTER_SOURCE_DIR").is_none() {
        let accepted = rfd::MessageDialog::new()
            .set_title("L2TV Electron版データ引継ぎツール")
            .set_description(
                "Electron版L2TVを終了してから、旧L2TVフォルダを選択します。\n旧データは変更せず、Tauri版で読み込めるJSONだけを書き出します。",
            )
            .set_buttons(rfd::MessageButtons::OkCancel)
            .set_level(rfd::MessageLevel::Info)
            .show();
        if !matches!(accepted, rfd::MessageDialogResult::Ok) {
            return Ok(None);
        }
    }

    let selected = match std::env::var_os("L2TV_EXPORTER_SOURCE_DIR") {
        Some(path) => PathBuf::from(path),
        None => {
            let mut dialog = rfd::FileDialog::new()
                .set_title("旧Electron版のL2TVフォルダ、またはlr2ir-table-lamp-viewer-dataを選択");
            if let Ok(executable) = std::env::current_exe()
                && let Some(parent) = executable.parent()
            {
                dialog = dialog.set_directory(parent);
            }
            let Some(path) = dialog.pick_folder() else {
                return Ok(None);
            };
            path
        }
    };

    let portable_data = resolve_portable_data_directory(&selected).ok_or_else(|| {
        format!(
            "選択した場所に旧Electron版の保存データが見つかりません。\n旧L2TVの解凍先、{PORTABLE_DATA_DIR}、またはElectronの保存領域を選択してください。"
        )
    })?;
    let storage_root = resolve_storage_root(&portable_data).ok_or_else(|| {
        "旧Electron版のIndexedDBまたはLocal Storageが見つかりません。\n一度も読み込みを行っていない場合は、引き継ぐデータがありません。".to_string()
    })?;
    let origin = discover_legacy_origin(&storage_root).unwrap_or(LegacyOrigin {
        host: DEFAULT_HOST.to_string(),
        port: DEFAULT_PORT,
        stored_bytes: 0,
    });

    let output_path = match std::env::var_os("L2TV_EXPORTER_OUTPUT") {
        Some(path) => PathBuf::from(path),
        None => {
            let file_name = format!(
                "L2TV_data_transfer_{}.json",
                chrono::Local::now().format("%Y%m%d_%H%M%S")
            );
            let Some(path) = rfd::FileDialog::new()
                .set_title("Tauri版へ読み込む引継ぎJSONの保存先")
                .set_file_name(&file_name)
                .add_filter("L2TV data transfer", &["json"])
                .save_file()
            else {
                return Ok(None);
            };
            path
        }
    };
    if output_path == portable_data || output_path.is_dir() {
        return Err("引継ぎJSONの保存先が正しくありません。".to_string());
    }

    let profile_directory = temporary_profile_directory();
    prepare_webview_profile(&storage_root, &profile_directory)?;
    let listener = TcpListener::bind((DEFAULT_HOST, origin.port)).map_err(|error| {
        cleanup_profile(&profile_directory);
        format!(
            "旧Electron版と同じポート{}を使用できません。\nElectron版L2TVを完全に終了してから、もう一度実行してください。\n{error}",
            origin.port
        )
    })?;

    Ok(Some(ExportPlan {
        output_path,
        profile_directory,
        origin,
        listener,
        auto_exit: std::env::var_os("L2TV_EXPORTER_AUTO_EXIT").is_some(),
    }))
}

fn resolve_portable_data_directory(selected: &Path) -> Option<PathBuf> {
    let direct = selected.join(PORTABLE_DATA_DIR);
    if direct.is_dir() {
        return Some(direct);
    }
    if selected
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(PORTABLE_DATA_DIR))
        && selected.is_dir()
    {
        return Some(selected.to_path_buf());
    }
    if selected.file_name().is_some_and(|name| {
        name.eq_ignore_ascii_case("session-data") || name.eq_ignore_ascii_case("user-data")
    }) {
        return selected.parent().map(Path::to_path_buf);
    }
    if selected.join("IndexedDB").is_dir()
        || selected.join("Local Storage").is_dir()
        || selected.join("Default").join("IndexedDB").is_dir()
        || selected.join("Default").join("Local Storage").is_dir()
    {
        return Some(selected.to_path_buf());
    }
    None
}

fn resolve_storage_root(portable_data: &Path) -> Option<PathBuf> {
    let candidates = [
        portable_data.join("session-data"),
        portable_data.join("user-data"),
        portable_data.join("session-data").join("Default"),
        portable_data.join("user-data").join("Default"),
        portable_data.join("Default"),
        portable_data.to_path_buf(),
    ];
    candidates.into_iter().find(|candidate| {
        candidate.join("IndexedDB").is_dir() || candidate.join("Local Storage").is_dir()
    })
}

fn discover_legacy_origin(storage_root: &Path) -> Option<LegacyOrigin> {
    let indexed_db = storage_root.join("IndexedDB");
    let mut origins = Vec::new();
    let entries = fs::read_dir(indexed_db).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(base) = name.strip_suffix(".indexeddb.leveldb") else {
            continue;
        };
        let Some(rest) = base.strip_prefix("http_") else {
            continue;
        };
        let Some((host, port)) = rest.rsplit_once('_') else {
            continue;
        };
        let Ok(port) = port.parse::<u16>() else {
            continue;
        };
        if !matches!(host, "127.0.0.1" | "localhost") {
            continue;
        }
        origins.push(LegacyOrigin {
            host: host.to_string(),
            port,
            stored_bytes: directory_size(&path),
        });
    }
    origins.sort_by(|left, right| {
        let left_preferred = left.port == DEFAULT_PORT;
        let right_preferred = right.port == DEFAULT_PORT;
        right_preferred
            .cmp(&left_preferred)
            .then_with(|| right.stored_bytes.cmp(&left.stored_bytes))
    });
    origins.into_iter().next()
}

fn prepare_webview_profile(storage_root: &Path, profile_directory: &Path) -> Result<(), String> {
    let target_default = profile_directory.join("EBWebView").join("Default");
    fs::create_dir_all(&target_default).map_err(|error| error.to_string())?;
    let mut copied_any = false;
    let mut copied_bytes = 0_u64;
    for name in ["IndexedDB", "Local Storage"] {
        let source = storage_root.join(name);
        if !source.is_dir() {
            continue;
        }
        copy_directory(&source, &target_default.join(name), &mut copied_bytes)?;
        copied_any = true;
    }
    if !copied_any {
        cleanup_profile(profile_directory);
        return Err("旧Electron版の保存データをコピーできませんでした。".to_string());
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path, copied_bytes: &mut u64) -> Result<(), String> {
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            continue;
        }
        if !entry.file_type().is_file() || entry.file_name().eq_ignore_ascii_case("LOCK") {
            continue;
        }
        let length = entry.metadata().map_err(|error| error.to_string())?.len();
        *copied_bytes = copied_bytes.saturating_add(length);
        if *copied_bytes > MAX_PROFILE_COPY_BYTES {
            return Err("旧保存データが大きすぎるため、安全のため処理を中止しました。".to_string());
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(entry.path(), &target).map_err(|error| {
            format!(
                "旧保存データを読み取れませんでした。Electron版L2TVが終了しているか確認してください。\n{error}"
            )
        })?;
    }
    Ok(())
}

fn directory_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok().map(|metadata| metadata.len()))
        .sum()
}

fn temporary_profile_directory() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "l2tv-electron-exporter-{}-{timestamp}",
        std::process::id()
    ))
}

fn start_server(listener: TcpListener, state: Arc<ExportServerState>) {
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                break;
            };
            let state = Arc::clone(&state);
            thread::spawn(move || {
                let _ = handle_connection(stream, &state);
            });
        }
    });
}

fn handle_connection(mut stream: TcpStream, state: &ExportServerState) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    let (method, path, body) = read_request(&mut stream)?;
    match (method.as_str(), path.as_str()) {
        ("GET", "/") => write_response(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            EXPORTER_HTML.as_bytes(),
        ),
        ("GET", "/favicon.ico") => write_response(&mut stream, 204, "image/x-icon", &[]),
        ("POST", "/export") => {
            let response = match write_transfer_payload(&state.output_path, &body) {
                Ok(bytes) => {
                    if state.auto_exit {
                        let app = state.app.clone();
                        thread::spawn(move || {
                            thread::sleep(Duration::from_millis(600));
                            app.exit(0);
                        });
                    }
                    serde_json::to_vec(&json!({
                        "ok": true,
                        "filePath": state.output_path.to_string_lossy(),
                        "bytes": bytes,
                    }))
                    .unwrap_or_else(|_| b"{\"ok\":true}".to_vec())
                }
                Err(error) => serde_json::to_vec(&json!({ "ok": false, "error": error }))
                    .unwrap_or_else(|_| b"{\"ok\":false}".to_vec()),
            };
            write_response(
                &mut stream,
                200,
                "application/json; charset=utf-8",
                &response,
            )
        }
        _ => write_response(&mut stream, 404, "text/plain; charset=utf-8", b"Not Found"),
    }
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<(String, String, Vec<u8>)> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let header_end = loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "request ended before headers",
            ));
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > 64 * 1024 {
            return Err(std::io::Error::other("request headers are too large"));
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = headers.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_EXPORT_BYTES {
        return Err(std::io::Error::other("request body is too large"));
    }
    while bytes.len().saturating_sub(header_end) < content_length {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len().saturating_sub(header_end) > MAX_EXPORT_BYTES {
            return Err(std::io::Error::other("request body is too large"));
        }
    }
    if bytes.len().saturating_sub(header_end) < content_length {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "request body is incomplete",
        ));
    }
    Ok((
        method,
        path,
        bytes[header_end..header_end + content_length].to_vec(),
    ))
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn write_transfer_payload(output_path: &Path, body: &[u8]) -> Result<usize, String> {
    if body.is_empty() || body.len() > MAX_EXPORT_BYTES {
        return Err("引継ぎデータが空か、大きすぎます。".to_string());
    }
    let payload: Value = serde_json::from_slice(body)
        .map_err(|_| "引継ぎデータの形式が正しくありません。".to_string())?;
    validate_transfer_payload(&payload)?;
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|error| error.to_string())?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(output_path)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(bytes.len())
}

fn validate_transfer_payload(payload: &Value) -> Result<(), String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "引継ぎデータの形式が正しくありません。".to_string())?;
    if object.get("format").and_then(Value::as_str) != Some("l2tv-data-transfer")
        || object.get("version").and_then(Value::as_u64) != Some(1)
    {
        return Err("引継ぎデータの形式が正しくありません。".to_string());
    }
    let data = object
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| "引継ぎデータの形式が正しくありません。".to_string())?;
    if !DATA_TRANSFER_KEYS.iter().any(|key| data.contains_key(*key)) {
        return Err("引継ぎ対象の保存データがありません。".to_string());
    }
    Ok(())
}

fn cleanup_profile(path: &Path) {
    for _ in 0..20 {
        if !path.exists() || fs::remove_dir_all(path).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(150));
    }
}

fn show_error(message: &str) {
    rfd::MessageDialog::new()
        .set_title("L2TV Electron版データ引継ぎツール")
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .set_level(rfd::MessageLevel::Error)
        .show();
}

const EXPORTER_HTML: &str = r#"<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>L2TV Electron版データ引継ぎツール</title>
  <style>
    :root { color-scheme: dark; font-family: "Segoe UI", "Yu Gothic UI", sans-serif; }
    * { box-sizing: border-box; }
    body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: #0b111b; color: #e8f2ff; }
    main { width: min(620px, calc(100% - 40px)); padding: 32px; border: 1px solid #41536d; background: #111a28; box-shadow: 0 18px 60px #0008; }
    .eyebrow { color: #65c9ff; font-size: 12px; font-weight: 700; letter-spacing: 0; text-transform: uppercase; }
    h1 { margin: 8px 0 12px; font-size: 25px; }
    p { margin: 0; color: #b9c6d8; line-height: 1.7; }
    .status { margin-top: 24px; padding: 18px; border-left: 4px solid #65c9ff; background: #0c1420; }
    .status strong { display: block; margin-bottom: 6px; color: #fff; }
    .status.success { border-color: #61e89b; }
    .status.error { border-color: #ff7d8b; }
    .path { margin-top: 10px; color: #d9f4ff; overflow-wrap: anywhere; }
    .note { margin-top: 20px; font-size: 13px; }
  </style>
</head>
<body>
  <main>
    <div class="eyebrow">L2TV 3.0.0</div>
    <h1>Electron版データを抽出しています</h1>
    <p>旧保存領域のコピーから、Tauri版で読み込める引継ぎJSONを作成します。元データは変更しません。</p>
    <div id="status" class="status" role="status" aria-live="polite">
      <strong>読み取り中</strong>
      <span>保存データを確認しています。しばらくお待ちください。</span>
    </div>
    <p class="note">完了後、L2TVのメニューから「引継ぎデータを読み込む」を選択してください。</p>
  </main>
  <script>
    const DB_NAME = "lr2ir-table-lamp-viewer";
    const STORE_NAME = "app-state";
    const PREFIX = `${DB_NAME}:`;
    const KEYS = [
      "form-state",
      "last-analysis",
      "table-preset-selection",
      "custom-table-list-entries",
      "stellaverse-rival-ids"
    ];
    const status = document.getElementById("status");

    function setStatus(kind, title, message, path = "") {
      status.className = `status ${kind}`;
      status.replaceChildren();
      const strong = document.createElement("strong");
      strong.textContent = title;
      const text = document.createElement("span");
      text.textContent = message;
      status.append(strong, text);
      if (path) {
        const pathNode = document.createElement("div");
        pathNode.className = "path";
        pathNode.textContent = path;
        status.append(pathNode);
      }
    }

    function openDatabase() {
      return new Promise((resolve) => {
        const request = indexedDB.open(DB_NAME);
        let created = false;
        request.onupgradeneeded = () => {
          created = true;
          request.transaction.abort();
        };
        request.onsuccess = () => resolve(created ? null : request.result);
        request.onerror = () => resolve(null);
        request.onblocked = () => resolve(null);
      });
    }

    async function readIndexedValues() {
      const db = await openDatabase();
      if (!db || !db.objectStoreNames.contains(STORE_NAME)) return { found: 0, data: {} };
      const data = {};
      let found = 0;
      await Promise.all(KEYS.map((key) => new Promise((resolve) => {
        const transaction = db.transaction(STORE_NAME, "readonly");
        const request = transaction.objectStore(STORE_NAME).get(key);
        request.onsuccess = () => {
          const record = request.result;
          if (record && record.version === 1) {
            data[key] = record.value ?? null;
            found += 1;
          }
          resolve();
        };
        request.onerror = () => resolve();
      })));
      db.close();
      return { found, data };
    }

    function readLocalValue(key) {
      try {
        const raw = localStorage.getItem(`${PREFIX}${key}`);
        if (!raw) return { found: false, value: null };
        const record = JSON.parse(raw);
        return record && record.version === 1
          ? { found: true, value: record.value ?? null }
          : { found: false, value: null };
      } catch {
        return { found: false, value: null };
      }
    }

    (async () => {
      try {
        const indexed = await readIndexedValues();
        const data = { ...indexed.data };
        let found = indexed.found;
        for (const key of KEYS) {
          if (Object.prototype.hasOwnProperty.call(data, key)) continue;
          const local = readLocalValue(key);
          if (local.found) {
            data[key] = local.value;
            found += 1;
          } else {
            data[key] = null;
          }
        }
        if (!found) throw new Error("引継ぎ対象の保存データが見つかりませんでした。選択したフォルダを確認してください。");
        const payload = {
          format: "l2tv-data-transfer",
          version: 1,
          exportedAt: new Date().toISOString(),
          appVersion: "Electron legacy",
          data
        };
        const response = await fetch("/export", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload)
        });
        const result = await response.json();
        if (!result.ok) throw new Error(result.error || "JSONを書き出せませんでした。");
        setStatus("success", "書き出しが完了しました", "このウィンドウを閉じて、Tauri版L2TVからJSONを読み込んでください。", result.filePath);
      } catch (error) {
        setStatus("error", "書き出しに失敗しました", error instanceof Error ? error.message : String(error));
      }
    })();
  </script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::{
        DATA_TRANSFER_KEYS, LegacyOrigin, discover_legacy_origin, resolve_portable_data_directory,
        validate_transfer_payload,
    };
    use serde_json::json;
    use std::fs;

    #[test]
    fn resolves_old_l2tv_root_and_data_directory() {
        let root =
            std::env::temp_dir().join(format!("l2tv-exporter-path-test-{}", std::process::id()));
        let data = root.join("lr2ir-table-lamp-viewer-data");
        fs::create_dir_all(&data).unwrap();
        assert_eq!(resolve_portable_data_directory(&root), Some(data.clone()));
        assert_eq!(resolve_portable_data_directory(&data), Some(data.clone()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_direct_electron_profile_directory() {
        let root =
            std::env::temp_dir().join(format!("l2tv-exporter-profile-test-{}", std::process::id()));
        fs::create_dir_all(root.join("Default").join("IndexedDB")).unwrap();
        assert_eq!(resolve_portable_data_directory(&root), Some(root.clone()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_electron_loopback_indexeddb_origin() {
        let root =
            std::env::temp_dir().join(format!("l2tv-exporter-origin-test-{}", std::process::id()));
        let database = root
            .join("IndexedDB")
            .join("http_127.0.0.1_4173.indexeddb.leveldb");
        fs::create_dir_all(&database).unwrap();
        fs::write(database.join("000003.log"), b"fixture").unwrap();
        assert_eq!(
            discover_legacy_origin(&root),
            Some(LegacyOrigin {
                host: "127.0.0.1".into(),
                port: 4173,
                stored_bytes: 7,
            })
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validates_exporter_payload_contract() {
        let payload = json!({
            "format": "l2tv-data-transfer",
            "version": 1,
            "data": { DATA_TRANSFER_KEYS[0]: { "language": "ja" } }
        });
        assert!(validate_transfer_payload(&payload).is_ok());
        assert!(validate_transfer_payload(&json!({ "format": "wrong" })).is_err());
    }
}
