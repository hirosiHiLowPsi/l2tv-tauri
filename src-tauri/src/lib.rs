mod api;
mod beatoraja_random;
mod commands;
mod database;
mod electron_transfer_exporter;
mod error;
mod external;
mod openlr2_random;
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
    if electron_transfer_exporter::should_run() {
        electron_transfer_exporter::run();
        return;
    }

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
                if std::env::var_os("L2TV_E2E_MARKER").is_some() {
                    match _payload.event() {
                        tauri::webview::PageLoadEvent::Started => {
                            let _ = _window.eval("window.__L2TV_E2E__ = true;");
                        }
                        tauri::webview::PageLoadEvent::Finished => {
                            if let Some(script) = e2e_initialization_script() {
                                let _ = _window.eval(&script);
                            }
                        }
                    }
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
            commands::open_beatoraja_calendar,
            commands::beatoraja_calendar_data,
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
    let game_data_mode = serde_json::to_string(
        &std::env::var("L2TV_E2E_GAME_DATA_MODE").unwrap_or_else(|_| "lr2".into()),
    )
    .ok()?;
    Some(format!(
        r#"
window.__L2TV_E2E__ = true;
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
      songDbPath: {song_path},
      gameDataMode: {game_data_mode}
    }});
    const profile = await bridge.requestApi("/api/profile-from-db", {{
      scoreDbPath: {score_path},
      songDbPath: {song_path},
      gameDataMode: {game_data_mode},
      scoreDbMode: {game_data_mode} === "beatoraja" ? "legacy" : "stellaverse",
      skillAnalyzerFetchMode: "both",
      allowStellaverseNetwork: false
    }});
    const analysis = await bridge.requestApi("/api/analyze", {{
      scoreDbPath: {score_path},
      songDbPath: {song_path},
      gameDataMode: {game_data_mode},
      scoreDbMode: {game_data_mode} === "beatoraja" ? "legacy" : "stellaverse",
      skillAnalyzerFetchMode: "both",
      allowStellaverseNetwork: false,
      includeUnlistedUpdates: false,
      tableUrls: []
    }});
    const history = {game_data_mode} === "beatoraja"
      ? await bridge.requestApi("/api/beatoraja-history", {{
          scoreDbPath: {score_path},
          songDbPath: {song_path},
          year: new Date().getFullYear()
        }})
      : null;
    const calendarWindow = {game_data_mode} === "beatoraja"
      ? await bridge.openBeatorajaCalendar({{
          scoreDbPath: {score_path},
          songDbPath: {song_path},
          language: "ja",
          theme: "lr2ir-dark"
        }})
      : null;
    const calendarBridgeData = calendarWindow?.opened
      ? await bridge.getBeatorajaCalendarData({{
          year: Number(history?.year) || new Date().getFullYear()
        }})
      : null;
    const scoreEntries = Array.isArray(analysis?.localScoreState?.entries)
      ? analysis.localScoreState.entries
      : [];
    const optionEntries = scoreEntries.filter((entry) => entry?.playOption);
    const randomLayoutEntry = optionEntries.find((entry) => /^[1-7]{{7}}$/.test(entry?.randomLayout || ""));
    const rRandomLayoutEntry = optionEntries.find(
      (entry) => entry?.playOption === "R-RANDOM" && /^[1-7]{{7}}$/.test(entry?.randomLayout || "")
    );
    const testHooks = window.__l2tvE2e;
    if (!testHooks) {{
      throw new Error("L2TV E2E test hooks are unavailable");
    }}
    const scoreInput = document.getElementById("score-db-path");
    const songInput = document.getElementById("song-db-path");
    scoreInput.value = {score_path};
    songInput.value = {song_path};
    testHooks.renderAnalysisForE2e(analysis, {game_data_mode});
    const profilePanel = document.querySelector('#results-root > .overview-panel');
    const embeddedCalendar = document.querySelector('#results-root > .beatoraja-calendar-inline');
    const embeddedCalendarFrame = embeddedCalendar?.querySelector(".beatoraja-calendar-frame");
    if (embeddedCalendarFrame) {{
      for (let attempt = 0; attempt < 30; attempt += 1) {{
        if (embeddedCalendarFrame.contentDocument?.body?.classList.contains("calendar-embedded")) {{
          break;
        }}
        await new Promise((resolve) => setTimeout(resolve, 100));
      }}
      for (let attempt = 0; attempt < 50; attempt += 1) {{
        const summary = embeddedCalendarFrame.contentDocument?.getElementById("calendar-summary")?.textContent || "";
        if (summary && !summary.includes("scoredatalog.db")) {{
          break;
        }}
        await new Promise((resolve) => setTimeout(resolve, 100));
      }}
    }}
    const keyboard = testHooks.createRandomLayoutKeyboard("1743265");
    const renderedKeyboardOrder = [...keyboard.children]
      .map((key) => key.dataset.lane || "")
      .join("");
    const renderedKeyboardImages = [...keyboard.children]
      .map((key) => new URL(key.src).pathname.split("/").pop() || "")
      .join(",");
    const randomOption = testHooks.describeScoreRandomOption("RANDOM / EASY");
    const rRandomOption = testHooks.describeScoreRandomOption("R-RANDOM");
    const normalOption = testHooks.describeScoreRandomOption("HARD");
    const mirrorOption = testHooks.describeScoreRandomOption("MIRROR / HARD");
    const sRandomOption = testHooks.describeScoreRandomOption("S-RANDOM / EASY");
    const hRandomOption = testHooks.describeScoreRandomOption("H-RANDOM / DEATH");
    const lr2DatabasePaths = testHooks.setDatabasePathsForMode(
      {{}}, "lr2", "lr2-score.db", "lr2-song.db"
    );
    const separatedDatabasePaths = testHooks.setDatabasePathsForMode(
      lr2DatabasePaths, "beatoraja", "oraja-score.db", "oraja-songdata.db"
    );
    scoreInput.value = "lr2-live-score.db";
    songInput.value = "lr2-live-song.db";
    testHooks.captureDatabasePathsForMode("lr2");
    scoreInput.value = "oraja-live-score.db";
    songInput.value = "oraja-live-songdata.db";
    testHooks.captureDatabasePathsForMode("beatoraja");
    testHooks.applyDatabasePathsForMode("lr2");
    const restoredLr2Paths = [scoreInput.value, songInput.value];
    testHooks.applyDatabasePathsForMode("beatoraja");
    const restoredBeatorajaPaths = [scoreInput.value, songInput.value];
    result = {{
      ok: true,
      readyState: document.readyState,
      bridge: typeof bridge?.requestApi,
      transferBridge: typeof bridge?.exportDataTransfer === "function" && typeof bridge?.importDataTransfer === "function",
      transferButtonsEnabled:
        !document.getElementById("export-transfer-button")?.disabled &&
        !document.getElementById("import-transfer-button")?.disabled,
      menuOpened,
      rivalFeaturesAvailable: testHooks.areRivalFeaturesAvailable(),
      rivalToggleHidden: document.getElementById("rival-toggle-button")?.classList.contains("hidden") === true,
      rivalFolderHidden: document.getElementById("rival-folder-field")?.classList.contains("hidden") === true,
      rivalColumnAvailable: testHooks.getVisibleChartSortColumns().some((column) => column.key === "rival"),
      embeddedCalendarPresent: Boolean(embeddedCalendar),
      embeddedCalendarAfterProfile: Boolean(profilePanel && profilePanel.nextElementSibling === embeddedCalendar),
      embeddedCalendarLoaded:
        embeddedCalendarFrame?.contentDocument?.body?.classList.contains("calendar-embedded") === true,
      embeddedCalendarDataLoaded: Boolean(
        embeddedCalendarFrame?.contentDocument?.getElementById("calendar-year-title")?.textContent
      ),
      embeddedCalendarReadyState: embeddedCalendarFrame?.contentDocument?.readyState || "",
      embeddedCalendarUrl: embeddedCalendarFrame?.contentWindow?.location?.href || "",
      embeddedCalendarBodyClass: embeddedCalendarFrame?.contentDocument?.body?.className || "",
      embeddedCalendarBodyText: embeddedCalendarFrame?.contentDocument?.body?.innerText?.slice(0, 160) || "",
      scoreDbExists: state.scoreDb.exists,
      gameDataMode: profile.player.gameDataMode,
      name: profile.player.name,
      id: profile.player.lr2Id,
      grade: profile.player.gradeSp,
      st: profile.player.skillAnalyzer?.st?.grade,
      triple: profile.player.overjoyTripleCrown,
      optionEntryCount: optionEntries.length,
      randomLayoutSample: randomLayoutEntry?.randomLayout || "",
      randomLayoutOption: randomLayoutEntry?.playOption || "",
      rRandomLayoutSample: rRandomLayoutEntry?.randomLayout || "",
      rRandomLayoutOption: rRandomLayoutEntry?.playOption || "",
      renderedKeyboardOrder,
      renderedKeyboardImages,
      renderedKeyboardHasVisibleText: keyboard.textContent !== "",
      scoreOptionDisplayValid:
        randomOption.label === "乱" && randomOption.type === "random" &&
        rRandomOption.label === "R乱" && rRandomOption.type === "r-random" &&
        normalOption.label === "正規" && normalOption.type === "normal" &&
        mirrorOption.label === "鏡" && mirrorOption.type === "mirror" &&
        sRandomOption.label === "S乱" && sRandomOption.type === "s-random" &&
        hRandomOption.label === "H乱" && hRandomOption.type === "h-random",
      fixedKeyboardLayoutsValid:
        testHooks.getScoreOptionKeyboardLayout({{ keyMode: 7 }}, "normal") === "1234567" &&
        testHooks.getScoreOptionKeyboardLayout({{ keyMode: 7 }}, "mirror") === "7654321" &&
        testHooks.getScoreOptionKeyboardLayout(
          {{ keyMode: 7, randomLayout: "1743265" }},
          "random"
        ) === "1743265" &&
        testHooks.getScoreOptionKeyboardLayout(
          {{ keyMode: 7, randomLayout: "1765432" }},
          "r-random"
        ) === "1765432" &&
        testHooks.getScoreOptionKeyboardLayout({{ keyMode: 7 }}, "s-random") === "",
      lr2AnalysisKey: testHooks.getLastAnalysisKeyForMode("lr2"),
      beatorajaAnalysisKey: testHooks.getLastAnalysisKeyForMode("beatoraja"),
      analysisModesSeparated:
        testHooks.getAnalysisGameDataMode({{ player: {{ gameDataMode: "lr2" }} }}) === "lr2" &&
        testHooks.getAnalysisGameDataMode({{ player: {{ gameDataMode: "beatoraja" }} }}) === "beatoraja",
      databasePathsSeparated:
        separatedDatabasePaths.lr2.scoreDbPath === "lr2-score.db" &&
        separatedDatabasePaths.lr2.songDbPath === "lr2-song.db" &&
        separatedDatabasePaths.beatoraja.scoreDbPath === "oraja-score.db" &&
        separatedDatabasePaths.beatoraja.songDbPath === "oraja-songdata.db",
      databasePathSwitchRoundTrip:
        restoredLr2Paths.join("|") === "lr2-live-score.db|lr2-live-song.db" &&
        restoredBeatorajaPaths.join("|") === "oraja-live-score.db|oraja-live-songdata.db",
      historyDayCount: Array.isArray(history?.days) ? history.days.length : 0,
      historyTotalPlays: Number(history?.totalPlays || 0),
      historyHasYearList: Array.isArray(history?.availableYears),
      calendarWindowOpened: calendarWindow?.opened === true,
      calendarBridgeDayCount: Array.isArray(calendarBridgeData?.days)
        ? calendarBridgeData.days.length
        : 0,
      calendarBridgeSource: calendarBridgeData?.sourceFileName || "",
      calendarBridgeTheme: calendarBridgeData?.theme || ""
    }};
  }} catch (error) {{
    result = {{ ok: false, error: String(error?.message || error) }};
  }}
  await window.__TAURI__.core.invoke("write_e2e_marker", {{ payload: result }});
}})();
"#
    ))
}
