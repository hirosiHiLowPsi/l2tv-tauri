mod api;
mod commands;
mod database;
mod error;
mod external;
mod security;
mod stellaverse;
mod tables;

use api::AppState;
/// Developer-facing parity hook used by the non-bundled API probe example.
pub async fn probe_api(path: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    api::request(path, body, &AppState::new())
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .setup(|app| {
            let window_builder = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("L2TV")
            .inner_size(1400.0, 900.0)
            .min_inner_size(960.0, 640.0)
            .resizable(true)
            .center()
            .disable_drag_drop_handler()
            .on_page_load(|_window, _payload| {
                #[cfg(debug_assertions)]
                if matches!(_payload.event(), tauri::webview::PageLoadEvent::Finished)
                    && let Some(script) = e2e_initialization_script()
                {
                    let _ = _window.eval(&script);
                }
            })
            .on_navigation(|url| {
                if external::is_app_url(url) {
                    true
                } else {
                    external::open_external_url(url);
                    false
                }
            })
            .on_new_window(|url, _features| {
                external::open_external_url(&url);
                tauri::webview::NewWindowResponse::Deny
            });
            let window_builder = match std::env::var_os("L2TV_WEBVIEW_DATA_DIR") {
                Some(path) => window_builder.data_directory(std::path::PathBuf::from(path)),
                None => window_builder,
            };
            let window = window_builder.build()?;
            window.show()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::api_request,
            commands::pick_file,
            commands::pick_directory,
            commands::save_image,
            commands::export_data_transfer,
            commands::import_data_transfer,
            commands::fetch_stellaverse_rival,
            commands::fetch_stellaverse_rankings,
            commands::write_e2e_marker,
        ])
        .run(tauri::generate_context!())
        .expect("L2TV Tauri could not start");
}

#[cfg(debug_assertions)]
fn e2e_initialization_script() -> Option<String> {
    std::env::var_os("L2TV_E2E_MARKER")?;
    let score_path =
        serde_json::to_string(&std::env::var("L2TV_E2E_SCORE_DB").unwrap_or_default()).ok()?;
    let song_path =
        serde_json::to_string(&std::env::var("L2TV_E2E_SONG_DB").unwrap_or_default()).ok()?;
    Some(format!(
        r#"
(async () => {{
  await new Promise((resolve) => setTimeout(resolve, 100));
  let result;
  try {{
    if (document.querySelector(".language-startup-modal")) {{
      document.querySelector(".language-startup-actions button")?.click();
      await new Promise((resolve) => setTimeout(resolve, 50));
    }}
    const menuButton = document.getElementById("menu-toggle-button");
    menuButton?.click();
    const drawer = document.getElementById("control-drawer");
    const menuOpened = Boolean(drawer && !drawer.classList.contains("hidden"));
    document.getElementById("menu-close-button")?.click();
    const bridge = window.lr2irDesktop;
    const state = await bridge.requestApi("/api/local-db-state", {{
      scoreDbPath: {score_path},
      songDbPath: {song_path}
    }});
    const profile = await bridge.requestApi("/api/profile-from-db", {{
      scoreDbPath: {score_path},
      songDbPath: {song_path},
      scoreDbMode: "stellaverse",
      skillAnalyzerFetchMode: "both",
      allowStellaverseNetwork: false
    }});
    result = {{
      ok: true,
      readyState: document.readyState,
      bridge: typeof bridge?.requestApi,
      transferBridge: typeof bridge?.exportDataTransfer === "function" && typeof bridge?.importDataTransfer === "function",
      transferButtonsEnabled:
        !document.getElementById("export-transfer-button")?.disabled &&
        !document.getElementById("import-transfer-button")?.disabled,
      menuOpened,
      scoreDbExists: state.scoreDb.exists,
      name: profile.player.name,
      id: profile.player.lr2Id,
      grade: profile.player.gradeSp,
      st: profile.player.skillAnalyzer?.st?.grade,
      triple: profile.player.overjoyTripleCrown
    }};
  }} catch (error) {{
    result = {{ ok: false, error: String(error?.message || error) }};
  }}
  await window.__TAURI__.core.invoke("write_e2e_marker", {{ payload: result }});
}})();
"#
    ))
}
