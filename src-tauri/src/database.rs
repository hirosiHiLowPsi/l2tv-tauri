use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use encoding_rs::SHIFT_JIS;
use regex::Regex;
use rusqlite::{Connection, OpenFlags, Row, params};
use serde::Serialize;
use serde_json::{Map, Number, Value, json};

use crate::api::AppState;
use crate::beatoraja_random::{
    r_random_layout_for_display_7k as beatoraja_r_random_layout_for_display_7k,
    random_layout_for_display_7k as beatoraja_random_layout_for_display_7k,
};
use crate::error::{Result, message};
use crate::openlr2_random::random_layout_for_display_7k;
use crate::stellaverse;
use crate::tables;

const FORCE_CONSTANTS_JSON: &str = include_str!("../../public/data/force-chart-constants.json");
const COURSE_HASHES_JSON: &str = include_str!("../../public/data/local-course-hashes.json");
const IR_RANK_INDEX_JSON: &str = include_str!("../../public/data/ir-rank-top100.json");

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScoreEntry {
    md5: String,
    title: String,
    artist: String,
    lamp_status: String,
    play_count: Option<i64>,
    ex_score: Option<i64>,
    max_ex_score: Option<i64>,
    score_rate: Option<f64>,
    max_offset: Option<i64>,
    bad_count: Option<i64>,
    poor_count: Option<i64>,
    miss_count: Option<i64>,
    play_option: Option<String>,
    option_code: Option<i64>,
    random_seed: Option<i64>,
    random_layout: Option<String>,
    key_mode: Option<i64>,
}

#[derive(Clone, Debug, Default)]
struct SongInfo {
    md5: String,
    title: String,
    artist: String,
    key_mode: Option<i64>,
}

#[derive(Clone, Debug, Default)]
struct Lr2SongCatalog {
    by_md5: HashMap<String, SongInfo>,
    md5_hashes: HashSet<String>,
    loaded: bool,
}

#[derive(Clone, Debug, Default)]
struct BeatorajaSongCatalog {
    by_sha256: HashMap<String, SongInfo>,
    md5_hashes: HashSet<String>,
}

#[derive(Clone, Debug, Default)]
struct Profile {
    player_id: String,
    name: String,
    lr2_id: String,
    grade: String,
    grade_sp: String,
    grade_dp: String,
    skill_analyzer: Value,
    overjoy_triple_crown: bool,
    force_dan_candidate: Option<Value>,
    hit_totals: Value,
    play_time_total: Value,
}

#[derive(Clone, Debug, Default)]
struct PlayerData {
    score_db_path: String,
    song_db_path: String,
    game_data_mode: String,
    score_db_mode: String,
    profile: Profile,
    entries: Vec<ScoreEntry>,
    by_hash: HashMap<String, ScoreEntry>,
    local_song_hashes: HashSet<String>,
    has_song_catalog: bool,
}

#[derive(Clone, Debug, Default)]
struct RivalData {
    path: String,
    players: Vec<Value>,
    by_hash: HashMap<String, Vec<Value>>,
}

#[derive(Clone, Debug, Default)]
struct IrRankIndex {
    limit: i64,
    charts: HashMap<String, IrRankEntry>,
}

#[derive(Clone, Debug, Default)]
struct IrRankEntry {
    total_players: i64,
    scores: Vec<i64>,
}

#[derive(Clone, Debug)]
struct DanDefinition {
    level: u8,
    label: &'static str,
    grade: &'static str,
    course_id: u32,
    constant: f64,
}

const DAN_DEFINITIONS: &[DanDefinition] = &[
    DanDefinition {
        level: 11,
        label: "発狂初段",
        grade: "★1",
        course_id: 11110,
        constant: 4.29,
    },
    DanDefinition {
        level: 12,
        label: "発狂二段",
        grade: "★2",
        course_id: 11109,
        constant: 6.24,
    },
    DanDefinition {
        level: 13,
        label: "発狂三段",
        grade: "★3",
        course_id: 11108,
        constant: 8.32,
    },
    DanDefinition {
        level: 14,
        label: "発狂四段",
        grade: "★4",
        course_id: 11107,
        constant: 9.72,
    },
    DanDefinition {
        level: 15,
        label: "発狂五段",
        grade: "★5",
        course_id: 11106,
        constant: 12.40,
    },
    DanDefinition {
        level: 16,
        label: "発狂六段",
        grade: "★6",
        course_id: 11105,
        constant: 14.22,
    },
    DanDefinition {
        level: 17,
        label: "発狂七段",
        grade: "★7",
        course_id: 11104,
        constant: 17.28,
    },
    DanDefinition {
        level: 18,
        label: "発狂八段",
        grade: "★8",
        course_id: 11103,
        constant: 18.68,
    },
    DanDefinition {
        level: 19,
        label: "発狂九段",
        grade: "★9",
        course_id: 11102,
        constant: 21.35,
    },
    DanDefinition {
        level: 20,
        label: "発狂十段",
        grade: "★10",
        course_id: 11101,
        constant: 23.41,
    },
    DanDefinition {
        level: 21,
        label: "発狂皆伝",
        grade: "★★",
        course_id: 11100,
        constant: 24.44,
    },
    DanDefinition {
        level: 22,
        label: "Overjoy",
        grade: "(^^)",
        course_id: 11099,
        constant: 26.81,
    },
];

pub async fn local_db_state(body: Value) -> Result<Value> {
    let score = body
        .get("scoreDbPath")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let game_data_mode = normalize_game_data_mode(
        body.get("gameDataMode")
            .and_then(Value::as_str)
            .unwrap_or("lr2"),
        Path::new(score),
    );
    let song = resolve_song_db_path(
        score,
        body.get("songDbPath")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        &game_data_mode,
    );
    Ok(json!({
        "fetchedAt": chrono::Utc::now().to_rfc3339(),
        "gameDataMode": game_data_mode,
        "scoreDb": file_state(score).await,
        "songDb": file_state(song.to_string_lossy().as_ref()).await,
    }))
}

pub async fn profile_from_db(body: Value, state: &AppState) -> Result<Value> {
    let score_path = clean_path(
        body.get("scoreDbPath")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    if score_path.as_os_str().is_empty() {
        return Err(message("score.db パスを入力してください。"));
    }
    let game_data_mode = normalize_game_data_mode(
        body.get("gameDataMode")
            .and_then(Value::as_str)
            .unwrap_or("lr2"),
        &score_path,
    );
    let song_path = resolve_song_db_path(
        score_path.to_string_lossy().as_ref(),
        body.get("songDbPath")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        &game_data_mode,
    );
    let mode = normalize_score_db_mode(
        body.get("scoreDbMode")
            .and_then(Value::as_str)
            .unwrap_or("auto"),
        &score_path,
    );
    let should_fetch_stellaverse_name = mode == "stellaverse"
        && body
            .get("allowStellaverseNetwork")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let skill_mode = normalize_skill_mode(
        body.get("skillAnalyzerFetchMode")
            .and_then(Value::as_str)
            .unwrap_or("both"),
    );
    let mut player = tokio::task::spawn_blocking(move || {
        load_player_data(&score_path, &song_path, &mode, &skill_mode, &game_data_mode)
    })
    .await
    .map_err(|error| message(error.to_string()))??;
    apply_stellaverse_name(&mut player, should_fetch_stellaverse_name, state).await;
    Ok(profile_json(&player))
}

pub async fn analyze(body: Value, state: &AppState) -> Result<Value> {
    let score_path = clean_path(
        body.get("scoreDbPath")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    if score_path.as_os_str().is_empty() {
        return Err(message("score.db のパスを入力してください。"));
    }
    let game_data_mode = normalize_game_data_mode(
        body.get("gameDataMode")
            .and_then(Value::as_str)
            .unwrap_or("lr2"),
        &score_path,
    );
    let song_path = resolve_song_db_path(
        score_path.to_string_lossy().as_ref(),
        body.get("songDbPath")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        &game_data_mode,
    );
    let rival_path = clean_path(
        body.get("rivalFolderPath")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mode = normalize_score_db_mode(
        body.get("scoreDbMode")
            .and_then(Value::as_str)
            .unwrap_or("auto"),
        &score_path,
    );
    let should_fetch_stellaverse_name = mode == "stellaverse"
        && body
            .get("allowStellaverseNetwork")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let skill_mode = normalize_skill_mode(
        body.get("skillAnalyzerFetchMode")
            .and_then(Value::as_str)
            .unwrap_or("both"),
    );
    let include_unlisted = body
        .get("includeUnlistedUpdates")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let table_urls: Vec<String> = body
        .get("tableUrls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut loaded_tables = Vec::new();
    let mut table_errors = Vec::new();
    for url in &table_urls {
        match tables::load_table(url, state).await {
            Ok(table) => loaded_tables.push(table),
            Err(error) => table_errors.push(json!({ "tableUrl": url, "error": error.to_string() })),
        }
    }
    if !table_urls.is_empty() && loaded_tables.is_empty() {
        return Err(message("難易度表を1件も読み込めませんでした。"));
    }

    let mut player = tokio::task::spawn_blocking(move || {
        load_player_data(&score_path, &song_path, &mode, &skill_mode, &game_data_mode)
    })
    .await
    .map_err(|error| message(error.to_string()))??;
    apply_stellaverse_name(&mut player, should_fetch_stellaverse_name, state).await;
    let rival = tokio::task::spawn_blocking(move || load_rival_data(&rival_path))
        .await
        .map_err(|error| message(error.to_string()))??;

    let enriched_tables: Vec<Value> = loaded_tables
        .into_iter()
        .map(|table| enrich_table(table, &player, &rival))
        .collect();
    let ir_rankings = summarize_embedded_ir_rankings(&enriched_tables);
    let local_state = json!({
        "scoreDb": sync_file_state(&player.score_db_path),
        "songDb": sync_file_state(&player.song_db_path),
    });
    let local_score_state = json!({
        "entries": player.entries.iter().map(|entry| json!({
            "key": format!("unlisted:{}", entry.md5),
            "md5": entry.md5,
            "lampStatus": entry.lamp_status,
            "exScore": entry.ex_score,
            "scoreRate": entry.score_rate,
            "missCount": entry.miss_count,
            "playOption": entry.play_option,
            "randomLayout": entry.random_layout,
        })).collect::<Vec<_>>()
    });
    let unlisted = if include_unlisted {
        build_unlisted(&player, &enriched_tables)
    } else {
        Vec::new()
    };
    let force_rating = build_force_rating(&player);
    let overall = overall_summary(&enriched_tables);

    Ok(json!({
        "analyzedAt": chrono::Utc::now().to_rfc3339(),
        "localDbState": local_state,
        "localScoreState": local_score_state,
        "overall": overall,
        "player": {
            "id": if player.profile.player_id.is_empty() { "local" } else { &player.profile.player_id },
            "sourceType": "local-score-db",
            "gameDataMode": player.game_data_mode,
            "scoreDbMode": player.score_db_mode,
            "name": player.profile.name,
            "lr2Id": player.profile.lr2_id,
            "grade": player.profile.grade,
            "gradeSp": player.profile.grade_sp,
            "gradeDp": player.profile.grade_dp,
            "skillAnalyzer": player.profile.skill_analyzer,
            "stellaSkill4th": player.profile.skill_analyzer.get("st").cloned().unwrap_or(Value::Null),
            "overjoyTripleCrown": player.profile.overjoy_triple_crown,
            "forceRating": force_rating,
            "localDbPath": player.score_db_path,
            "localSongDbPath": player.song_db_path,
            "hitTotals": player.profile.hit_totals,
            "playTimeTotal": player.profile.play_time_total,
        },
        "rivals": {
            "folderPath": rival.path,
            "count": rival.players.len(),
            "totalScores": rival.players.iter().filter_map(|item| item.get("scoreCount").and_then(Value::as_u64)).sum::<u64>(),
            "players": rival.players,
        },
        "tableErrors": table_errors,
        "irRankings": ir_rankings,
        "tables": enriched_tables,
        "unlistedUpdateCharts": unlisted,
    }))
}

pub async fn beatoraja_history(body: Value) -> Result<Value> {
    let score_path = clean_path(
        body.get("scoreDbPath")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    if score_path.as_os_str().is_empty() {
        return Err(message("beatoraja score.db のパスを入力してください。"));
    }
    let song_path = resolve_song_db_path(
        score_path.to_string_lossy().as_ref(),
        body.get("songDbPath")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "beatoraja",
    );
    let current_year = Local::now().year();
    let requested_year = body
        .get("year")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| (2000..=2100).contains(value))
        .unwrap_or(current_year);
    let selected_date = body
        .get("date")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    tokio::task::spawn_blocking(move || {
        load_beatoraja_history(
            &score_path,
            &song_path,
            requested_year,
            selected_date.as_deref(),
        )
    })
    .await
    .map_err(|error| message(error.to_string()))?
}

#[derive(Default)]
struct BeatorajaHistoryDay {
    play_count: usize,
    chart_hashes: HashSet<String>,
}

fn load_beatoraja_history(
    score_path: &Path,
    song_path: &Path,
    requested_year: i32,
    selected_date: Option<&str>,
) -> Result<Value> {
    if !score_path.is_file() {
        return Err(message("指定された beatoraja score.db が見つかりません。"));
    }
    if !song_path.is_file() {
        return Err(message(
            "指定された beatoraja songdata.db が見つかりません。",
        ));
    }
    let history_path = score_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("scoredatalog.db");
    if !history_path.is_file() {
        return Err(message(
            "score.db と同じフォルダに scoredatalog.db が見つかりません。",
        ));
    }

    let history_database = open_readonly(&history_path)?;
    if !table_has_column(&history_database, "scoredatalog", "sha256")
        || !table_has_column(&history_database, "scoredatalog", "date")
    {
        return Err(message(
            "選択した scoredatalog.db はbeatoraja形式ではありません。",
        ));
    }
    let legacy_latest_only = table_has_primary_key(&history_database, "scoredatalog");
    let song_catalog = load_beatoraja_song_catalog(song_path)?;
    let available_years = beatoraja_history_years(&history_database)?;
    let (start_timestamp, end_timestamp) = local_year_bounds(requested_year)?;
    let history_rows = query_json_rows_between(
        &history_database,
        "SELECT sha256, mode, clear, epg, lpg, egr, lgr, egd, lgd, ebd, lbd, \
         epr, lpr, ems, lms, notes, combo, minbp, option, seed, date, state \
         FROM scoredatalog WHERE date >= ?1 AND date < ?2 ORDER BY date ASC",
        start_timestamp,
        end_timestamp,
    )?;

    let selected_date = selected_date
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| message("履歴の日付形式が正しくありません。"))
        })
        .transpose()?
        .filter(|value| value.year() == requested_year);
    let selected_date_text = selected_date.map(|value| value.format("%Y-%m-%d").to_string());
    let mut days: BTreeMap<String, BeatorajaHistoryDay> = BTreeMap::new();
    let mut entries = Vec::new();
    let mut layout_cache: HashMap<(String, i64, i64), String> = HashMap::new();

    for row in history_rows {
        let Some(timestamp) = get_i64(&row, "date") else {
            continue;
        };
        let Some(local_time) = Local.timestamp_opt(timestamp, 0).single() else {
            continue;
        };
        let day_key = local_time.format("%Y-%m-%d").to_string();
        let sha256 = valid_hex(get_text(&row, "sha256"), 64);
        let day = days.entry(day_key.clone()).or_default();
        day.play_count += 1;
        if !sha256.is_empty() {
            day.chart_hashes.insert(sha256.clone());
        }

        if selected_date_text.as_deref() != Some(day_key.as_str()) {
            continue;
        }
        let song = song_catalog.by_sha256.get(&sha256);
        let perfect = sum_present(get_i64(&row, "epg"), get_i64(&row, "lpg"));
        let great = sum_present(get_i64(&row, "egr"), get_i64(&row, "lgr"));
        let (ex_score, max_ex_score, score_rate, max_offset) =
            score_values(perfect, great, get_i64(&row, "notes"), Some(1));
        let option_code = get_i64(&row, "option");
        let option_info = beatoraja_score_option_info(
            &sha256,
            Some(1),
            option_code,
            get_i64(&row, "seed"),
            song.and_then(|value| value.key_mode),
            &mut layout_cache,
        );
        entries.push(json!({
            "timestamp": timestamp,
            "time": local_time.format("%H:%M:%S").to_string(),
            "title": song.map(|value| value.title.as_str()).filter(|value| !value.is_empty()).unwrap_or("Unknown Chart"),
            "artist": song.map(|value| value.artist.as_str()).unwrap_or_default(),
            "lampStatus": beatoraja_lamp_status(get_i64(&row, "clear"), Some(1)),
            "exScore": ex_score,
            "maxExScore": max_ex_score,
            "scoreRate": score_rate,
            "maxOffset": max_offset,
            "missCount": get_i64(&row, "minbp"),
            "playOption": option_info.play_option,
            "randomLayout": option_info.random_layout,
            "keyMode": song.and_then(|value| value.key_mode),
        }));
    }
    entries.reverse();

    let latest_date = days.keys().next_back().cloned();
    let total_plays = days.values().map(|day| day.play_count).sum::<usize>();
    let days = days
        .into_iter()
        .map(|(date, day)| {
            json!({
                "date": date,
                "playCount": day.play_count,
                "chartCount": day.chart_hashes.len(),
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "year": requested_year,
        "availableYears": available_years,
        "days": days,
        "latestDate": latest_date,
        "selectedDate": selected_date_text,
        "entries": entries,
        "totalPlays": total_plays,
        "legacyLatestOnly": legacy_latest_only,
        "sourceFileName": "scoredatalog.db",
    }))
}

fn local_year_bounds(year: i32) -> Result<(i64, i64)> {
    let start = Local
        .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
        .single()
        .ok_or_else(|| message("履歴の年を処理できませんでした。"))?;
    let end = Local
        .with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
        .single()
        .ok_or_else(|| message("履歴の年を処理できませんでした。"))?;
    Ok((start.timestamp(), end.timestamp()))
}

fn beatoraja_history_years(database: &Connection) -> Result<Vec<i32>> {
    let mut statement = database.prepare(
        "SELECT DISTINCT CAST(strftime('%Y', date, 'unixepoch', 'localtime') AS INTEGER) AS year \
         FROM scoredatalog WHERE date > 0 ORDER BY year DESC",
    )?;
    let years = statement.query_map([], |row| row.get::<_, Option<i32>>(0))?;
    Ok(years
        .filter_map(std::result::Result::ok)
        .flatten()
        .filter(|year| (2000..=2100).contains(year))
        .collect())
}

async fn apply_stellaverse_name(player: &mut PlayerData, enabled: bool, state: &AppState) {
    if !enabled || player.profile.lr2_id.is_empty() {
        return;
    }
    if let Ok(profile) = stellaverse::fetch_player_profile(&player.profile.lr2_id, state).await
        && let Some(name) = profile.get("name").and_then(Value::as_str)
        && !name.trim().is_empty()
    {
        player.profile.name = name.trim().to_string();
    }
}

fn load_player_data(
    score_path: &Path,
    song_path: &Path,
    mode: &str,
    skill_mode: &str,
    game_data_mode: &str,
) -> Result<PlayerData> {
    match game_data_mode {
        "beatoraja" => load_beatoraja_player_data(score_path, song_path, mode),
        _ => load_lr2_player_data(score_path, song_path, mode, skill_mode),
    }
}

fn load_lr2_player_data(
    score_path: &Path,
    song_path: &Path,
    mode: &str,
    skill_mode: &str,
) -> Result<PlayerData> {
    if !score_path.is_file() {
        return Err(message("指定された LR2 score.db が見つかりません。"));
    }
    let database = open_readonly(score_path)?;
    let player_rows = query_json_rows(&database, "SELECT * FROM player LIMIT 1")?;
    let player_row = player_rows
        .first()
        .cloned()
        .ok_or_else(|| message("score.db の player テーブルを読み取れませんでした。"))?;
    let score_rows = query_json_rows(&database, "SELECT * FROM score")?;
    let song_catalog = load_song_catalog(song_path);

    let mut entries = Vec::new();
    let mut by_hash = HashMap::new();
    let mut random_layout_cache: HashMap<(String, i64, i64), String> = HashMap::new();
    for row in &score_rows {
        let md5 = valid_hex(get_text(row, "hash"), 32);
        if md5.is_empty() || by_hash.contains_key(&md5) {
            continue;
        }
        let play_count = get_i64(row, "playcount");
        let perfect = get_i64(row, "perfect");
        let great = get_i64(row, "great");
        let total_notes = get_i64(row, "totalnotes");
        let bad = get_i64(row, "bad");
        let poor = get_i64(row, "poor");
        let min_bp = get_i64(row, "minbp");
        let miss_count = if play_count.unwrap_or(0) > 0 {
            min_bp.or_else(|| sum_options(bad, poor))
        } else {
            None
        };
        let (ex_score, max_ex_score, score_rate, max_offset) = if play_count.unwrap_or(0) > 0 {
            match (perfect, great, total_notes) {
                (Some(perfect), Some(great), Some(notes)) if notes > 0 => {
                    let ex = perfect * 2 + great;
                    let max = notes * 2;
                    (
                        Some(ex),
                        Some(max),
                        Some(round2(ex as f64 / max as f64 * 100.0)),
                        Some(max.saturating_sub(ex)),
                    )
                }
                _ => (None, None, None, None),
            }
        } else {
            (None, None, None, None)
        };
        let info = song_catalog.by_md5.get(&md5).cloned().unwrap_or_default();
        let option_code = get_i64(row, "op_best");
        let random_seed = get_i64(row, "rseed");
        let option_info = lr2_score_option_info(
            &md5,
            play_count,
            option_code,
            random_seed,
            info.key_mode,
            &mut random_layout_cache,
        );
        let entry = ScoreEntry {
            md5: md5.clone(),
            title: info.title,
            artist: info.artist,
            lamp_status: lamp_status(get_i64(row, "clear"), play_count),
            play_count,
            ex_score,
            max_ex_score,
            score_rate,
            max_offset,
            bad_count: bad,
            poor_count: poor,
            miss_count,
            play_option: option_info.play_option,
            option_code,
            random_seed,
            random_layout: option_info.random_layout,
            key_mode: info.key_mode,
        };
        by_hash.insert(md5, entry.clone());
        entries.push(entry);
    }

    let inferred = infer_grades(song_path, &score_rows);
    let fallback_sp = format_grade(get_i64(&player_row, "grade_7"));
    let grade_sp = inferred.0.unwrap_or(fallback_sp);
    let grade_dp = format_grade(get_i64(&player_row, "grade_14"));
    let skill_analyzer = build_skill_analyzer_progress(song_path, &score_rows, skill_mode);
    let profile_name = sanitize_player_name(&get_text(&player_row, "name"));
    let player_id = get_text(&player_row, "id");
    let lr2_id = first_text(
        &player_row,
        &["irid", "ir_id", "user_id", "player_id", "id"],
    );
    let force_dan_candidate = inferred
        .3
        .and_then(|level| build_force_dan_candidate(level, &score_rows, song_path));
    let hit_totals = hit_totals(&player_row);
    let play_time_total = play_time_total(&player_row);

    Ok(PlayerData {
        score_db_path: score_path.to_string_lossy().into_owned(),
        song_db_path: if song_path.is_file() {
            song_path.to_string_lossy().into_owned()
        } else {
            String::new()
        },
        game_data_mode: "lr2".into(),
        score_db_mode: mode.to_string(),
        profile: Profile {
            player_id,
            name: profile_name,
            lr2_id,
            grade: combine_grades(&grade_sp, &grade_dp),
            grade_sp,
            grade_dp,
            skill_analyzer,
            overjoy_triple_crown: has_overjoy_triple_crown(song_path, &score_rows),
            force_dan_candidate,
            hit_totals,
            play_time_total,
        },
        entries,
        by_hash,
        local_song_hashes: song_catalog.md5_hashes,
        has_song_catalog: song_catalog.loaded,
    })
}

fn load_beatoraja_player_data(
    score_path: &Path,
    song_path: &Path,
    mode: &str,
) -> Result<PlayerData> {
    if !score_path.is_file() {
        return Err(message("指定された beatoraja score.db が見つかりません。"));
    }
    if !song_path.is_file() {
        return Err(message(
            "指定された beatoraja songdata.db が見つかりません。",
        ));
    }

    let database = open_readonly(score_path)?;
    if !table_has_column(&database, "score", "sha256") {
        return Err(message(
            "選択したscore.dbはbeatoraja形式ではありません。ゲームモードかDBパスを確認してください。",
        ));
    }
    let player_row = query_json_rows(&database, "SELECT * FROM player ORDER BY date DESC LIMIT 1")?
        .into_iter()
        .next()
        .unwrap_or_else(|| json!({}));
    let score_rows = query_json_rows(&database, "SELECT * FROM score WHERE mode = 0")?;
    let song_catalog = load_beatoraja_song_catalog(song_path)?;

    let mut by_hash = HashMap::new();
    let mut random_layout_cache: HashMap<(String, i64, i64), String> = HashMap::new();
    for row in &score_rows {
        let sha256 = valid_hex(get_text(row, "sha256"), 64);
        let Some(info) = song_catalog.by_sha256.get(&sha256) else {
            continue;
        };
        if info.md5.is_empty() {
            continue;
        }

        let play_count = get_i64(row, "playcount");
        let perfect = sum_present(get_i64(row, "epg"), get_i64(row, "lpg"));
        let great = sum_present(get_i64(row, "egr"), get_i64(row, "lgr"));
        let total_notes = get_i64(row, "notes");
        let bad = sum_present(get_i64(row, "ebd"), get_i64(row, "lbd"));
        let poor = sum_present(get_i64(row, "epr"), get_i64(row, "lpr"));
        let miss_count = if play_count.unwrap_or(0) > 0 {
            get_i64(row, "minbp").or_else(|| sum_options(bad, poor))
        } else {
            None
        };
        let (ex_score, max_ex_score, score_rate, max_offset) =
            score_values(perfect, great, total_notes, play_count);
        let option_code = get_i64(row, "option");
        let random_seed = get_i64(row, "seed");
        let option_info = beatoraja_score_option_info(
            &sha256,
            play_count,
            option_code,
            random_seed,
            info.key_mode,
            &mut random_layout_cache,
        );
        let entry = ScoreEntry {
            md5: info.md5.clone(),
            title: info.title.clone(),
            artist: info.artist.clone(),
            lamp_status: beatoraja_lamp_status(get_i64(row, "clear"), play_count),
            play_count,
            ex_score,
            max_ex_score,
            score_rate,
            max_offset,
            bad_count: bad,
            poor_count: poor,
            miss_count,
            play_option: option_info.play_option,
            option_code,
            random_seed,
            random_layout: option_info.random_layout,
            key_mode: info.key_mode,
        };

        let should_replace = by_hash.get(&info.md5).is_none_or(|current: &ScoreEntry| {
            entry.ex_score.unwrap_or(-1) > current.ex_score.unwrap_or(-1)
                || (entry.ex_score == current.ex_score
                    && beatoraja_lamp_rank(&entry.lamp_status)
                        < beatoraja_lamp_rank(&current.lamp_status))
        });
        if should_replace {
            by_hash.insert(info.md5.clone(), entry);
        }
    }
    let mut entries: Vec<ScoreEntry> = by_hash.values().cloned().collect();
    entries.sort_by(|left, right| left.md5.cmp(&right.md5));

    let profile_name = beatoraja_profile_name(score_path);
    Ok(PlayerData {
        score_db_path: score_path.to_string_lossy().into_owned(),
        song_db_path: song_path.to_string_lossy().into_owned(),
        game_data_mode: "beatoraja".into(),
        score_db_mode: mode.to_string(),
        profile: Profile {
            player_id: profile_name.clone(),
            name: profile_name,
            lr2_id: String::new(),
            grade: String::new(),
            grade_sp: String::new(),
            grade_dp: String::new(),
            skill_analyzer: Value::Null,
            overjoy_triple_crown: false,
            force_dan_candidate: None,
            hit_totals: beatoraja_hit_totals(&player_row),
            play_time_total: play_time_total(&player_row),
        },
        entries,
        by_hash,
        local_song_hashes: song_catalog.md5_hashes,
        has_song_catalog: true,
    })
}

fn profile_json(player: &PlayerData) -> Value {
    json!({
        "player": {
            "id": player.profile.player_id,
            "sourceType": "local-score-db",
            "gameDataMode": player.game_data_mode,
            "name": player.profile.name,
            "lr2Id": player.profile.lr2_id,
            "grade": player.profile.grade,
            "gradeSp": player.profile.grade_sp,
            "gradeDp": player.profile.grade_dp,
            "skillAnalyzer": player.profile.skill_analyzer,
            "stellaSkill4th": player.profile.skill_analyzer.get("st").cloned().unwrap_or(Value::Null),
            "overjoyTripleCrown": player.profile.overjoy_triple_crown,
            "scoreDbMode": player.score_db_mode,
            "localDbPath": player.score_db_path,
            "hitTotals": player.profile.hit_totals,
            "playTimeTotal": player.profile.play_time_total,
        }
    })
}

fn open_readonly(path: &Path) -> Result<Connection> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?)
}

fn query_json_rows(connection: &Connection, sql: &str) -> Result<Vec<Value>> {
    let mut statement = connection.prepare(sql)?;
    let names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    let rows = statement.query_map([], |row| Ok(row_to_json(row, &names)))?;
    Ok(rows.filter_map(std::result::Result::ok).collect())
}

fn query_json_rows_between(
    connection: &Connection,
    sql: &str,
    start: i64,
    end: i64,
) -> Result<Vec<Value>> {
    let mut statement = connection.prepare(sql)?;
    let names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    let rows = statement.query_map(params![start, end], |row| Ok(row_to_json(row, &names)))?;
    Ok(rows.filter_map(std::result::Result::ok).collect())
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> bool {
    let Ok(mut statement) = connection.prepare(&format!("PRAGMA table_info({table})")) else {
        return false;
    };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(1)) else {
        return false;
    };
    rows.filter_map(std::result::Result::ok)
        .any(|name| name.eq_ignore_ascii_case(column))
}

fn table_has_primary_key(connection: &Connection, table: &str) -> bool {
    let Ok(mut statement) = connection.prepare(&format!("PRAGMA table_info({table})")) else {
        return false;
    };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, i64>(5)) else {
        return false;
    };
    rows.filter_map(std::result::Result::ok)
        .any(|primary_key_order| primary_key_order > 0)
}

fn row_to_json(row: &Row<'_>, names: &[String]) -> Value {
    let mut object = Map::new();
    for (index, name) in names.iter().enumerate() {
        let value = match row.get_ref(index) {
            Ok(rusqlite::types::ValueRef::Null) | Err(_) => Value::Null,
            Ok(rusqlite::types::ValueRef::Integer(value)) => Value::Number(Number::from(value)),
            Ok(rusqlite::types::ValueRef::Real(value)) => Number::from_f64(value)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            Ok(rusqlite::types::ValueRef::Text(value)) => {
                Value::String(String::from_utf8_lossy(value).into_owned())
            }
            Ok(rusqlite::types::ValueRef::Blob(value)) => Value::String(hex::encode(value)),
        };
        object.insert(name.clone(), value);
    }
    Value::Object(object)
}

#[derive(Debug, Default)]
struct Lr2ScoreOptionInfo {
    play_option: Option<String>,
    random_layout: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct DecodedLr2Option {
    label: String,
    player_one_random: u8,
}

fn lr2_score_option_info(
    md5: &str,
    play_count: Option<i64>,
    option_code: Option<i64>,
    random_seed: Option<i64>,
    key_mode: Option<i64>,
    layout_cache: &mut HashMap<(String, i64, i64), String>,
) -> Lr2ScoreOptionInfo {
    if play_count.unwrap_or(0) <= 0 {
        return Lr2ScoreOptionInfo::default();
    }

    let Some(option_code) = option_code else {
        return Lr2ScoreOptionInfo {
            play_option: Some("UNKNOWN".into()),
            random_layout: None,
        };
    };
    let Some(decoded) = decode_lr2_option(option_code, key_mode) else {
        return Lr2ScoreOptionInfo {
            play_option: Some("UNKNOWN".into()),
            random_layout: None,
        };
    };

    let random_layout = if key_mode == Some(7) && decoded.player_one_random == 2 {
        match random_seed.and_then(|seed| i32::try_from(seed).ok()) {
            Some(seed) => {
                let cache_key = (md5.to_string(), option_code, i64::from(seed));
                Some(
                    layout_cache
                        .entry(cache_key)
                        .or_insert_with(|| random_layout_for_display_7k(seed))
                        .clone(),
                )
            }
            None => Some("UNKNOWN".into()),
        }
    } else {
        None
    };

    Lr2ScoreOptionInfo {
        play_option: Some(decoded.label),
        random_layout,
    }
}

fn decode_lr2_option(option_code: i64, key_mode: Option<i64>) -> Option<DecodedLr2Option> {
    if !(0..=9_999).contains(&option_code) {
        return None;
    }

    let gauge = (option_code % 10) as u8;
    let player_one_random = ((option_code / 10) % 10) as u8;
    let player_two_random = ((option_code / 100) % 10) as u8;
    let dp_flip = ((option_code / 1_000) % 10) as u8;
    if gauge > 5 || player_one_random > 5 || player_two_random > 5 || dp_flip > 1 {
        return None;
    }

    let is_double_play = key_mode.is_some_and(|mode| mode >= 10);
    if !is_double_play && (player_two_random != 0 || dp_flip != 0) {
        return None;
    }

    let mut labels = Vec::new();
    if is_double_play {
        if player_one_random != 0 {
            labels.push(format!("1P {}", lr2_random_label(player_one_random)?));
        }
        if player_two_random != 0 {
            labels.push(format!("2P {}", lr2_random_label(player_two_random)?));
        }
        if dp_flip != 0 {
            labels.push("FLIP".into());
        }
    } else if player_one_random != 0 {
        labels.push(lr2_random_label(player_one_random)?.into());
    }

    if gauge != 0 {
        labels.push(lr2_gauge_label(gauge)?.into());
    }
    if labels.is_empty() {
        labels.push("NONE".into());
    }

    Some(DecodedLr2Option {
        label: labels.join(" / "),
        player_one_random,
    })
}

fn lr2_random_label(value: u8) -> Option<&'static str> {
    match value {
        1 => Some("MIRROR"),
        2 => Some("RANDOM"),
        3 => Some("S-RANDOM"),
        4 => Some("H-RANDOM"),
        5 => Some("ALL-SCRATCH"),
        _ => None,
    }
}

fn lr2_gauge_label(value: u8) -> Option<&'static str> {
    match value {
        1 => Some("HARD"),
        2 => Some("DEATH"),
        3 => Some("EASY"),
        4 => Some("P-ATTACK"),
        5 => Some("G-ATTACK"),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct BeatorajaScoreOptionInfo {
    play_option: Option<String>,
    random_layout: Option<String>,
}

fn beatoraja_score_option_info(
    sha256: &str,
    play_count: Option<i64>,
    option_code: Option<i64>,
    random_seed: Option<i64>,
    key_mode: Option<i64>,
    layout_cache: &mut HashMap<(String, i64, i64), String>,
) -> BeatorajaScoreOptionInfo {
    if play_count.unwrap_or(0) <= 0 {
        return BeatorajaScoreOptionInfo::default();
    }

    let Some(option_code) = option_code else {
        return BeatorajaScoreOptionInfo {
            play_option: Some("UNKNOWN".into()),
            random_layout: None,
        };
    };
    let Some((label, player_one_random)) = decode_beatoraja_option(option_code, key_mode) else {
        return BeatorajaScoreOptionInfo {
            play_option: Some("UNKNOWN".into()),
            random_layout: None,
        };
    };

    let random_layout_generator: Option<fn(i64) -> String> = match (key_mode, player_one_random) {
        (Some(7), 2) => Some(beatoraja_random_layout_for_display_7k),
        (Some(7), 3) => Some(beatoraja_r_random_layout_for_display_7k),
        _ => None,
    };
    let random_layout = if let Some(generate_layout) = random_layout_generator {
        match random_seed {
            Some(seed) if seed >= 0 => {
                let cache_key = (sha256.to_string(), option_code, seed);
                Some(
                    layout_cache
                        .entry(cache_key)
                        .or_insert_with(|| generate_layout(seed))
                        .clone(),
                )
            }
            _ => Some("UNKNOWN".into()),
        }
    } else {
        None
    };

    BeatorajaScoreOptionInfo {
        play_option: Some(label),
        random_layout,
    }
}

fn decode_beatoraja_option(option_code: i64, key_mode: Option<i64>) -> Option<(String, u8)> {
    if option_code < 0 {
        return None;
    }

    let is_double_play = key_mode.is_some_and(|mode| mode >= 10);
    if !is_double_play {
        let option = u8::try_from(option_code).ok()?;
        return Some((beatoraja_random_label(option)?.into(), option));
    }

    if option_code > 399 {
        return None;
    }
    let player_one = (option_code % 10) as u8;
    let player_two = ((option_code / 10) % 10) as u8;
    let double_option = ((option_code / 100) % 10) as u8;
    let mut labels = Vec::new();
    if player_one != 0 {
        labels.push(format!("1P {}", beatoraja_random_label(player_one)?));
    }
    if player_two != 0 {
        labels.push(format!("2P {}", beatoraja_random_label(player_two)?));
    }
    match double_option {
        0 => {}
        1 => labels.push("FLIP".into()),
        2 => labels.push("BATTLE".into()),
        3 => labels.push("BATTLE-A".into()),
        _ => return None,
    }
    if labels.is_empty() {
        labels.push("NONE".into());
    }
    Some((labels.join(" / "), player_one))
}

fn beatoraja_random_label(value: u8) -> Option<&'static str> {
    match value {
        0 => Some("NONE"),
        1 => Some("MIRROR"),
        2 => Some("RANDOM"),
        3 => Some("R-RANDOM"),
        4 => Some("S-RANDOM"),
        5 => Some("SPIRAL"),
        6 => Some("H-RANDOM"),
        7 => Some("ALL-SCRATCH"),
        8 => Some("EX-RANDOM"),
        9 => Some("EX-S-RANDOM"),
        _ => None,
    }
}

fn load_song_catalog(path: &Path) -> Lr2SongCatalog {
    if !path.is_file() {
        return Lr2SongCatalog::default();
    }
    let Ok(database) = open_readonly(path) else {
        return Lr2SongCatalog::default();
    };
    let key_mode_column = lr2_song_key_mode_column(&database);
    let Ok(rows) = query_json_rows(
        &database,
        &format!("SELECT hash, title, subtitle, artist, {key_mode_column} FROM song"),
    ) else {
        return Lr2SongCatalog::default();
    };
    let mut catalog = Lr2SongCatalog {
        loaded: true,
        ..Lr2SongCatalog::default()
    };
    for row in rows {
        let md5 = valid_hex(get_text(&row, "hash"), 32);
        if md5.is_empty() {
            continue;
        }
        catalog.md5_hashes.insert(md5.clone());
        let title = format!("{}{}", get_text(&row, "title"), get_text(&row, "subtitle"))
            .trim()
            .to_string();
        catalog.by_md5.entry(md5.clone()).or_insert(SongInfo {
            md5,
            title,
            artist: get_text(&row, "artist"),
            key_mode: get_i64(&row, "keymode"),
        });
    }
    catalog
}

fn lr2_song_key_mode_column(database: &Connection) -> &'static str {
    if table_has_column(database, "song", "keymode") {
        "keymode"
    } else if table_has_column(database, "song", "mode") {
        "mode AS keymode"
    } else {
        "NULL AS keymode"
    }
}

fn load_beatoraja_song_catalog(path: &Path) -> Result<BeatorajaSongCatalog> {
    let database = open_readonly(path)?;
    if !table_has_column(&database, "song", "sha256") || !table_has_column(&database, "song", "md5")
    {
        return Err(message(
            "選択した楽曲DBはbeatorajaのsongdata.db形式ではありません。",
        ));
    }

    let mut statement = database.prepare(
        "SELECT md5, sha256, title, subtitle, artist, mode FROM song \
         WHERE length(md5) = 32 AND length(sha256) = 64",
    )?;
    let rows = statement.query_map([], |row| {
        let md5: String = row.get(0)?;
        let sha256: String = row.get(1)?;
        let title: Option<String> = row.get(2)?;
        let subtitle: Option<String> = row.get(3)?;
        let artist: Option<String> = row.get(4)?;
        let mode: Option<i64> = row.get(5)?;
        Ok((md5, sha256, title, subtitle, artist, mode))
    })?;

    let mut catalog = BeatorajaSongCatalog::default();
    for row in rows.filter_map(std::result::Result::ok) {
        let md5 = valid_hex(row.0, 32);
        let sha256 = valid_hex(row.1, 64);
        if md5.is_empty() || sha256.is_empty() {
            continue;
        }
        let title = format!("{}{}", row.2.unwrap_or_default(), row.3.unwrap_or_default())
            .trim()
            .to_string();
        catalog.md5_hashes.insert(md5.clone());
        catalog.by_sha256.entry(sha256).or_insert(SongInfo {
            md5,
            title,
            artist: row.4.unwrap_or_default().trim().to_string(),
            key_mode: row.5,
        });
    }
    Ok(catalog)
}

fn resolve_song_db_path(score_path: &str, explicit: &str, game_data_mode: &str) -> PathBuf {
    let explicit = clean_path(explicit);
    if !explicit.as_os_str().is_empty() {
        return explicit;
    }
    let score = clean_path(score_path);
    if game_data_mode == "beatoraja" {
        for ancestor in score.ancestors().skip(1) {
            let candidate = ancestor.join("songdata.db");
            if candidate.is_file() {
                return candidate;
            }
        }
        return score
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."))
            .join("songdata.db");
    }
    score
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .join("song.db")
}

fn clean_path(value: &str) -> PathBuf {
    PathBuf::from(value.trim().trim_matches(['"', '\'']))
}

fn normalize_game_data_mode(value: &str, score_path: &Path) -> String {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "beatoraja" | "oraja" => "beatoraja".into(),
        "lr2" => "lr2".into(),
        _ => {
            if score_path.is_file()
                && open_readonly(score_path)
                    .ok()
                    .is_some_and(|database| table_has_column(&database, "score", "sha256"))
            {
                "beatoraja".into()
            } else {
                "lr2".into()
            }
        }
    }
}

fn normalize_score_db_mode(value: &str, score_path: &Path) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "legacy" | "bms-ir" | "stellaverse" => normalized,
        _ => {
            let stem = score_path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !stem.is_empty() && stem.chars().all(|ch| ch.is_ascii_digit()) {
                "stellaverse".into()
            } else {
                "legacy".into()
            }
        }
    }
}

fn normalize_skill_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "st" => "st".into(),
        "sl" => "sl".into(),
        _ => "both".into(),
    }
}

fn get_text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(|value| match value {
            Value::String(text) => text.trim().to_string(),
            Value::Number(number) => number.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}

fn first_text(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .map(|key| get_text(value, key))
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

fn get_i64(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
        .filter(|value| *value >= 0)
}

fn valid_hex(value: String, length: usize) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() == length && normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
        normalized
    } else {
        String::new()
    }
}

fn sum_options(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    Some(left? + right?)
}

fn sum_present(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (None, None) => None,
        (left, right) => Some(left.unwrap_or(0) + right.unwrap_or(0)),
    }
}

fn score_values(
    perfect: Option<i64>,
    great: Option<i64>,
    total_notes: Option<i64>,
    play_count: Option<i64>,
) -> (Option<i64>, Option<i64>, Option<f64>, Option<i64>) {
    if play_count.unwrap_or(0) <= 0 {
        return (None, None, None, None);
    }
    match (perfect, great, total_notes) {
        (Some(perfect), Some(great), Some(notes)) if notes > 0 => {
            let ex = perfect * 2 + great;
            let max = notes * 2;
            (
                Some(ex),
                Some(max),
                Some(round2(ex as f64 / max as f64 * 100.0)),
                Some(max.saturating_sub(ex)),
            )
        }
        _ => (None, None, None, None),
    }
}

fn lamp_status(clear: Option<i64>, play_count: Option<i64>) -> String {
    match clear.unwrap_or(-1) {
        5 => "FULL COMBO",
        4 => "HARD CLEAR",
        3 => "CLEAR",
        2 => "EASY CLEAR",
        1 => "FAILED",
        0 => "NO PLAY",
        _ if play_count.unwrap_or(0) > 0 => "FAILED",
        _ => "NO PLAY",
    }
    .into()
}

fn beatoraja_lamp_status(clear: Option<i64>, play_count: Option<i64>) -> String {
    match clear.unwrap_or(-1) {
        10 => "MAX",
        9 => "PERFECT",
        8 => "FULL COMBO",
        7 => "EX HARD CLEAR",
        6 => "HARD CLEAR",
        5 => "CLEAR",
        4 => "EASY CLEAR",
        2 | 3 => "ASSIST",
        1 => "FAILED",
        0 => "NO PLAY",
        _ if play_count.unwrap_or(0) > 0 => "FAILED",
        _ => "NO PLAY",
    }
    .into()
}

fn beatoraja_lamp_rank(lamp: &str) -> usize {
    match lamp {
        "MAX" => 0,
        "PERFECT" => 1,
        "FULL COMBO" => 2,
        "EX HARD CLEAR" => 3,
        "HARD CLEAR" => 4,
        "CLEAR" => 5,
        "EASY CLEAR" => 6,
        "ASSIST" => 7,
        "FAILED" => 8,
        "NO PLAY" => 9,
        _ => 10,
    }
}

fn beatoraja_profile_name(score_path: &Path) -> String {
    let parent = score_path.parent();
    let parent_name = parent
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let looks_like_snapshot = parent_name.len() == 8
        && parent_name
            .chars()
            .all(|character| character.is_ascii_digit());
    let candidate = if looks_like_snapshot {
        parent.and_then(Path::parent)
    } else {
        parent
    };
    candidate
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("beatoraja player")
        .chars()
        .take(64)
        .collect()
}

fn format_grade(value: Option<i64>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    match value {
        1..=10 => format!("☆{value}"),
        11..=20 => format!("★{}", value - 10),
        21 => "★★".into(),
        22 => "(^^)".into(),
        101..=110 => format!("☆{}", value - 100),
        111..=120 => format!("★{}", value - 110),
        121 => "★★".into(),
        122 => "(^^)".into(),
        _ => String::new(),
    }
}

fn combine_grades(sp: &str, dp: &str) -> String {
    match (sp.is_empty(), dp.is_empty()) {
        (false, false) => format!("{sp}/{dp}"),
        (false, true) => sp.into(),
        (true, false) => dp.into(),
        _ => String::new(),
    }
}

fn sanitize_player_name(value: &str) -> String {
    let value = value.trim();
    if value.to_ascii_lowercase().ends_with(".db") || value.contains(['\\', '/']) {
        String::new()
    } else {
        value.chars().take(64).collect()
    }
}

fn infer_grades(
    song_path: &Path,
    score_rows: &[Value],
) -> (Option<String>, Option<u8>, Option<u8>, Option<u8>) {
    let mut courses = Vec::new();
    if song_path.is_file()
        && let Ok(database) = open_readonly(song_path)
        && let Ok(rows) = query_json_rows(&database, "SELECT title, hash FROM grade")
    {
        courses.extend(rows);
    }
    if let Ok(bundle) = serde_json::from_str::<Value>(COURSE_HASHES_JSON) {
        for key in [
            "genoside2018Sp",
            "stellaSkillSimulator4th",
            "satelliteSkillAnalyzer2nd",
        ] {
            courses.extend(
                bundle
                    .get(key)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
    }
    let score_by_hash = score_lookup(score_rows);
    let mut highest_normal: Option<(u8, String)> = None;
    let mut highest_insane = None;
    let mut highest_st = None;
    let mut highest_sl = None;
    for course in courses {
        let title = get_text(&course, "title");
        let hash = get_text(&course, "hash").to_ascii_lowercase();
        let passed = score_by_hash
            .get(&hash)
            .is_some_and(|row| get_i64(row, "clear").unwrap_or(0) >= 2);
        if !passed {
            continue;
        }
        if let Some((level, grade, insane)) = parse_genoside_grade(&title) {
            if insane {
                highest_insane = Some(highest_insane.unwrap_or(0).max(level));
            }
            if highest_normal
                .as_ref()
                .is_none_or(|(current, _)| level > *current)
            {
                highest_normal = Some((level, grade));
            }
        }
        if let Some((kind, level)) = parse_skill_level(&title) {
            if kind == "st" {
                highest_st = Some(highest_st.unwrap_or(0).max(level));
            }
            if kind == "sl" {
                highest_sl = Some(highest_sl.unwrap_or(0).max(level));
            }
        }
    }
    (
        highest_normal.map(|(_, grade)| grade),
        highest_st,
        highest_sl,
        highest_insane,
    )
}

fn parse_genoside_grade(title: &str) -> Option<(u8, String, bool)> {
    let compact = title
        .replace([' ', '\t', '\r', '\n'], "")
        .replace("GENOCIDE", "GENOSIDE");
    if !compact.to_ascii_lowercase().contains("genoside2018") || !compact.contains("段位認定") {
        return None;
    }
    if compact.to_ascii_lowercase().contains("overjoy") {
        return Some((22, "(^^)".into(), true));
    }
    if compact.contains("発狂皆伝") {
        return Some((21, "★★".into(), true));
    }
    if compact.contains("皆伝") {
        return Some((10, "☆10".into(), false));
    }
    let numerals = [
        ("初", 1),
        ("一", 1),
        ("二", 2),
        ("三", 3),
        ("四", 4),
        ("五", 5),
        ("六", 6),
        ("七", 7),
        ("八", 8),
        ("九", 9),
        ("十", 10),
    ];
    let insane = compact.contains("発狂");
    for (text, number) in numerals.iter().rev() {
        let needle = if insane {
            format!("発狂{text}段")
        } else {
            format!("{text}段")
        };
        if compact.contains(&needle) {
            let level = if insane { number + 10 } else { *number };
            let grade = if insane {
                format!("★{number}")
            } else {
                format!("☆{number}")
            };
            return Some((level, grade, insane));
        }
    }
    None
}

fn parse_skill_level(title: &str) -> Option<(&'static str, u8)> {
    let compact = title.replace(' ', "").to_ascii_lowercase();
    let st = Regex::new(r"st0*(\d{1,2})")
        .ok()?
        .captures(&compact)
        .and_then(|caps| caps[1].parse::<u8>().ok());
    if compact.contains("stellaskillsimulator4th") {
        return st.map(|level| ("st", level));
    }
    let sl = Regex::new(r"sl0*(\d{1,2})")
        .ok()?
        .captures(&compact)
        .and_then(|caps| caps[1].parse::<u8>().ok());
    if compact.contains("satelliteskillanalyzer2nd") {
        return sl.map(|level| ("sl", level));
    }
    None
}

fn score_lookup(rows: &[Value]) -> HashMap<String, Value> {
    let mut lookup = HashMap::new();
    for row in rows {
        for key in ["hash", "scorehash"] {
            let hash = get_text(row, key).to_ascii_lowercase();
            if !hash.is_empty() {
                lookup.entry(hash).or_insert_with(|| row.clone());
            }
        }
    }
    lookup
}

fn build_skill_analyzer_progress(song_path: &Path, score_rows: &[Value], mode: &str) -> Value {
    #[derive(Default)]
    struct Aggregate {
        total: usize,
        played: usize,
        cleared: usize,
        highest: Option<u8>,
    }

    let mut courses = Vec::new();
    if song_path.is_file()
        && let Ok(database) = open_readonly(song_path)
        && let Ok(rows) = query_json_rows(&database, "SELECT title, hash FROM grade")
    {
        courses.extend(rows);
    }
    if let Ok(bundle) = serde_json::from_str::<Value>(COURSE_HASHES_JSON) {
        for key in ["stellaSkillSimulator4th", "satelliteSkillAnalyzer2nd"] {
            courses.extend(
                bundle
                    .get(key)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
    }

    let mut groups = HashMap::<(String, u8), HashSet<String>>::new();
    for course in courses {
        let Some((kind, level)) = parse_skill_level(&get_text(&course, "title")) else {
            continue;
        };
        let hash = get_text(&course, "hash").to_ascii_lowercase();
        if !hash.is_empty() {
            groups
                .entry((kind.to_string(), level))
                .or_default()
                .insert(hash);
        }
    }

    let lookup = score_lookup(score_rows);
    let mut st = Aggregate::default();
    let mut sl = Aggregate::default();
    for ((kind, level), hashes) in groups {
        let aggregate = if kind == "st" { &mut st } else { &mut sl };
        aggregate.total += 1;
        let mut played = false;
        let mut passed = false;
        for hash in hashes {
            if let Some(row) = lookup.get(&hash) {
                played |= get_i64(row, "playcount").unwrap_or(0) > 0
                    || row.get("clear").is_some_and(|value| !value.is_null());
                passed |= get_i64(row, "clear").unwrap_or(0) >= 2;
            }
        }
        if played {
            aggregate.played += 1;
        }
        if passed {
            aggregate.cleared += 1;
            aggregate.highest = Some(aggregate.highest.unwrap_or(0).max(level));
        }
    }

    let mut value = Map::new();
    if mode != "sl" && st.total > 0 {
        value.insert(
            "st".into(),
            json!({
                "grade": st.highest.map(|level| format!("st{level}")).unwrap_or_default(),
                "formalName": "Stella Skill Simulator 4th",
                "clearedCount": st.cleared,
                "playedCount": st.played,
                "totalCount": st.total,
            }),
        );
    }
    if mode != "st" && sl.total > 0 {
        value.insert(
            "sl".into(),
            json!({
                "grade": sl.highest.map(|level| format!("sl{level}")).unwrap_or_default(),
                "formalName": "Satellite Skill Analyzer 2nd",
                "clearedCount": sl.cleared,
                "playedCount": sl.played,
                "totalCount": sl.total,
            }),
        );
    }
    if value.is_empty() {
        Value::Null
    } else {
        Value::Object(value)
    }
}

fn build_force_dan_candidate(level: u8, score_rows: &[Value], song_path: &Path) -> Option<Value> {
    let definition = DAN_DEFINITIONS.iter().find(|item| item.level == level)?;
    let state = find_course_score(definition.label, score_rows, song_path);
    let clear = state.as_ref().and_then(|row| get_i64(row, "clear"));
    let lamp = match clear.unwrap_or(3) {
        value if value >= 4 => "HARD CLEAR",
        value if value >= 2 => "CLEAR",
        _ => return None,
    };
    let score_ratio = state.as_ref().and_then(|row| {
        let perfect = get_i64(row, "perfect")?;
        let great = get_i64(row, "great")?;
        let notes = get_i64(row, "totalnotes")?;
        (notes > 0).then_some((perfect * 2 + great) as f64 / (notes * 2) as f64)
    });
    let coefficient = if definition.course_id == 11099 {
        1.0
    } else {
        score_ratio.map(force_score_coefficient).unwrap_or(1.0)
    };
    let force = definition.constant * coefficient;
    let ex_score = state
        .as_ref()
        .and_then(|row| Some(get_i64(row, "perfect")? * 2 + get_i64(row, "great")?));
    let max_ex_score = state
        .as_ref()
        .and_then(|row| Some(get_i64(row, "totalnotes")? * 2));
    let course_hash = state
        .as_ref()
        .map(|row| get_text(row, "_courseHash"))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("dan:{}", definition.course_id));
    Some(json!({
        "candidateType": "dan",
        "force": force,
        "chartConstant": definition.constant,
        "danConstant": definition.constant,
        "rankValue": definition.level,
        "grade": definition.grade,
        "title": format!("GENOSIDE2018 {}", definition.label),
        "label": definition.label,
        "courseId": definition.course_id,
        "md5": course_hash,
        "source": "dan",
        "lampStatus": lamp,
        "exScore": ex_score,
        "maxExScore": max_ex_score,
        "scoreRate": score_ratio.map(|ratio| ratio * 100.0),
        "scoreCoefficient": coefficient,
        "lampCoefficient": 1,
    }))
}

fn find_course_score(label: &str, score_rows: &[Value], song_path: &Path) -> Option<Value> {
    let lookup = score_lookup(score_rows);
    let database = open_readonly(song_path).ok()?;
    let courses = query_json_rows(&database, "SELECT title, hash FROM grade").ok()?;
    courses.into_iter().find_map(|course| {
        let title = get_text(&course, "title")
            .replace([' ', '\t', '\r', '\n'], "")
            .replace("GENOCIDE", "GENOSIDE");
        if !title.to_ascii_lowercase().contains("genoside2018")
            || !title.contains("段位認定")
            || !title.contains(&label.replace(' ', ""))
        {
            return None;
        }
        let hash = get_text(&course, "hash").to_ascii_lowercase();
        lookup.get(&hash).cloned().map(|mut row| {
            if let Some(object) = row.as_object_mut() {
                object.insert("_courseHash".into(), json!(hash));
                object.insert("_courseTitle".into(), json!(get_text(&course, "title")));
            }
            row
        })
    })
}

fn has_overjoy_triple_crown(song_path: &Path, score_rows: &[Value]) -> bool {
    if !song_path.is_file() {
        return false;
    }
    let Ok(database) = open_readonly(song_path) else {
        return false;
    };
    let Ok(courses) = query_json_rows(&database, "SELECT title, hash FROM grade") else {
        return false;
    };
    let lookup = score_lookup(score_rows);
    let target_titles = HashSet::from([
        "overjoy".to_string(),
        "段位認定overjoy".to_string(),
        "genoside2018段位認定overjoy".to_string(),
    ]);
    let mut found = HashSet::new();
    for course in courses {
        let title = normalize_grade_title_key(&get_text(&course, "title"));
        if target_titles.contains(&title)
            && lookup
                .get(&get_text(&course, "hash").to_ascii_lowercase())
                .is_some_and(|row| get_i64(row, "clear").unwrap_or(0) >= 2)
        {
            found.insert(title);
        }
    }
    found.len() == target_titles.len()
}

fn normalize_grade_title_key(title: &str) -> String {
    let compact = title
        .chars()
        .map(|character| match character {
            '\u{ff01}'..='\u{ff5e}' => {
                char::from_u32(character as u32 - 0xfee0).unwrap_or(character)
            }
            '\u{3000}' => ' ',
            _ => character,
        })
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    compact.replacen("genocide2018段位認定", "genoside2018段位認定", 1)
}

fn hit_totals(row: &Value) -> Value {
    let perfect = get_i64(row, "perfect").unwrap_or(0);
    let great = get_i64(row, "great").unwrap_or(0);
    let good = get_i64(row, "good").unwrap_or(0);
    let bad = get_i64(row, "bad").unwrap_or(0);
    let poor_raw = get_i64(row, "poor").unwrap_or(0);
    json!({ "perfect": perfect, "great": great, "good": good, "bad": bad, "poor": 0, "poorRaw": poor_raw, "poorMode": "excluded", "total": perfect + great + good + bad })
}

fn beatoraja_hit_totals(row: &Value) -> Value {
    let perfect = get_i64(row, "epg").unwrap_or(0) + get_i64(row, "lpg").unwrap_or(0);
    let great = get_i64(row, "egr").unwrap_or(0) + get_i64(row, "lgr").unwrap_or(0);
    let good = get_i64(row, "egd").unwrap_or(0) + get_i64(row, "lgd").unwrap_or(0);
    let bad = get_i64(row, "ebd").unwrap_or(0) + get_i64(row, "lbd").unwrap_or(0);
    let poor_raw = get_i64(row, "epr").unwrap_or(0) + get_i64(row, "lpr").unwrap_or(0);
    json!({
        "perfect": perfect,
        "great": great,
        "good": good,
        "bad": bad,
        "poor": 0,
        "poorRaw": poor_raw,
        "poorMode": "excluded",
        "total": perfect + great + good + bad,
    })
}

fn play_time_total(row: &Value) -> Value {
    let candidates = [
        "playtime",
        "play_time",
        "play_time_sec",
        "playtime_sec",
        "playseconds",
        "play_seconds",
        "totalplaytime",
        "total_play_time",
        "total_play_seconds",
    ];
    for key in candidates {
        if let Some(seconds) = get_i64(row, key) {
            return json!({ "totalSeconds": seconds, "sourceColumn": key });
        }
    }
    Value::Null
}

pub(crate) fn force_score_coefficient(ratio: f64) -> f64 {
    let ratio = ratio.clamp(0.0, 1.0);
    let value = if ratio < 8.0 / 9.0 {
        ratio
    } else if ratio < 0.9444 {
        0.9 + (0.98 - 0.9) * ((ratio - 8.0 / 9.0) / (0.9444 - 8.0 / 9.0))
    } else {
        0.98 + (1.0 - 0.98) * ((ratio - 0.9444) / (1.0 - 0.9444))
    };
    (value * 1000.0).round() / 1000.0
}

fn build_force_rating(player: &PlayerData) -> Value {
    let constants: Value = serde_json::from_str(FORCE_CONSTANTS_JSON).unwrap_or(Value::Null);
    let mut candidates = Vec::new();
    for chart in constants
        .get("charts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let md5 = get_text(chart, "md5").to_ascii_lowercase();
        let constant = chart
            .get("chartConstant")
            .and_then(Value::as_f64)
            .unwrap_or(-1.0);
        let Some(score) = player.by_hash.get(&md5) else {
            continue;
        };
        if !matches!(
            score.lamp_status.as_str(),
            "MAX"
                | "PERFECT"
                | "FULL COMBO"
                | "EX HARD CLEAR"
                | "HARD CLEAR"
                | "CLEAR"
                | "EASY CLEAR"
                | "ASSIST"
                | "FAILED"
        ) {
            continue;
        }
        let (Some(ex), Some(max)) = (score.ex_score, score.max_ex_score) else {
            continue;
        };
        if max <= 0 || constant < 0.0 {
            continue;
        }
        let coefficient = force_score_coefficient(ex as f64 / max as f64);
        candidates.push(json!({
            "candidateType": "chart", "force": constant * coefficient, "chartConstant": constant,
            "md5": md5, "source": get_text(chart, "source"), "sourceTable": get_text(chart, "sourceTable"),
            "difficulty": get_text(chart, "difficulty"), "title": score.title, "artist": score.artist,
            "lampStatus": score.lamp_status, "exScore": ex, "maxExScore": max, "scoreRate": score.score_rate,
            "scoreCoefficient": coefficient, "lampCoefficient": 1,
        }));
    }
    candidates.sort_by(|left, right| {
        right
            .get("force")
            .and_then(Value::as_f64)
            .partial_cmp(&left.get("force").and_then(Value::as_f64))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let played_charts = candidates.len();
    candidates.truncate(50);
    let best50_total: f64 = candidates
        .iter()
        .filter_map(|item| item.get("force").and_then(Value::as_f64))
        .sum();
    if let Some(dan) = &player.profile.force_dan_candidate {
        candidates.push(dan.clone());
    }
    candidates.sort_by(|left, right| {
        right
            .get("force")
            .and_then(Value::as_f64)
            .partial_cmp(&left.get("force").and_then(Value::as_f64))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let total: f64 = candidates
        .iter()
        .filter_map(|item| item.get("force").and_then(Value::as_f64))
        .sum();
    let rating = (total
        / if player.profile.force_dan_candidate.is_some() {
            51.0
        } else {
            50.0
        })
    .clamp(0.0, 30.0);
    let (title, tier) = force_tier(rating);
    for (index, item) in candidates.iter_mut().enumerate() {
        item.as_object_mut()
            .map(|object| object.insert("rank".into(), json!(index + 1)));
    }
    json!({
        "available": true, "rating": rating, "title": title, "tier": tier, "best50Total": best50_total,
        "top50Count": candidates.iter().filter(|item| item.get("candidateType").and_then(Value::as_str) == Some("chart")).count(),
        "broadCount": candidates.len(), "playedCharts": played_charts,
        "constantCharts": constants.get("charts").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "best20Average": if candidates.len() >= 20 { candidates.iter().take(20).filter_map(|item| item.get("force").and_then(Value::as_f64)).sum::<f64>() / 20.0 } else { rating },
        "best50Average": best50_total / 50.0, "broadAverage": rating,
        "danCandidate": player.profile.force_dan_candidate, "cutoff": candidates.last().and_then(|item| item.get("force")).cloned().unwrap_or(json!(0)),
        "topCharts": candidates,
    })
}

pub(crate) fn force_tier(rating: f64) -> (&'static str, &'static str) {
    if rating >= 25.0 {
        ("EVENT HORIZONE", "event-horizone")
    } else if rating >= 24.0 {
        ("SINGULARITY", "singularity")
    } else if rating >= 23.0 {
        ("ASTRAL IV", "astral-4")
    } else if rating >= 22.0 {
        ("ASTRAL III", "astral-3")
    } else if rating >= 21.0 {
        ("ASTRAL II", "astral-2")
    } else if rating >= 20.0 {
        ("ASTRAL I", "astral-1")
    } else if rating >= 19.0 {
        ("OBSIDIAN", "obsidian")
    } else if rating >= 18.0 {
        ("AURUM", "aurum")
    } else if rating >= 17.0 {
        ("ARGENT", "argent")
    } else if rating >= 16.0 {
        ("CRIMSON", "crimson")
    } else if rating >= 15.0 {
        ("AMETHYST", "amethyst")
    } else if rating >= 14.0 {
        ("JADE", "jade")
    } else if rating >= 12.0 {
        ("AMBER", "amber")
    } else if rating >= 10.0 {
        ("AZURE", "azure")
    } else {
        ("SLATE", "slate")
    }
}

fn load_rival_data(path: &Path) -> Result<RivalData> {
    if !path.is_dir() {
        return Ok(RivalData::default());
    }
    let mut data = RivalData {
        path: path.to_string_lossy().into_owned(),
        ..Default::default()
    };
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let db_path = entry.path();
        if db_path
            .extension()
            .and_then(|value| value.to_str())
            .is_none_or(|value| !value.eq_ignore_ascii_case("db"))
        {
            continue;
        }
        let id = db_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let name = load_rival_name(path, &id);
        let Ok(database) = open_readonly(&db_path) else {
            continue;
        };
        let Ok(rows) = query_json_rows(&database, "SELECT * FROM rival") else {
            continue;
        };
        let mut count = 0;
        for row in rows {
            let md5 = valid_hex(get_text(&row, "hash"), 32);
            if md5.is_empty() {
                continue;
            }
            let ex = sum_options(
                get_i64(&row, "r_perfect").map(|value| value * 2),
                get_i64(&row, "r_great"),
            );
            let max = get_i64(&row, "r_totalnotes").map(|value| value * 2);
            let score_rate = ex
                .zip(max)
                .filter(|(_, max)| *max > 0)
                .map(|(ex, max)| round2(ex as f64 / max as f64 * 100.0));
            let score = json!({
                "id": id, "name": name, "lampStatus": lamp_status(get_i64(&row, "r_clear"), Some(1)),
                "exScore": ex, "maxExScore": max, "scoreRate": score_rate,
                "missCount": get_i64(&row, "r_minbp").or_else(|| sum_options(get_i64(&row, "r_bad"), get_i64(&row, "r_poor"))),
            });
            data.by_hash.entry(md5).or_default().push(score);
            count += 1;
        }
        data.players
            .push(json!({ "id": id, "name": name, "scoreCount": count }));
    }
    Ok(data)
}

fn load_rival_name(directory: &Path, id: &str) -> String {
    let path = directory.join(format!("{id}.lr2folder"));
    let Ok(bytes) = fs::read(path) else {
        return id.into();
    };
    let (decoded, _, _) = SHIFT_JIS.decode(&bytes);
    decoded
        .lines()
        .find_map(|line| line.trim().strip_prefix("#TITLE ").map(str::trim))
        .filter(|value| !value.is_empty())
        .unwrap_or(id)
        .to_string()
}

fn embedded_ir_rank_index() -> &'static IrRankIndex {
    static INDEX: OnceLock<IrRankIndex> = OnceLock::new();
    INDEX.get_or_init(parse_embedded_ir_rank_index)
}

fn parse_embedded_ir_rank_index() -> IrRankIndex {
    let Ok(value) = serde_json::from_str::<Value>(IR_RANK_INDEX_JSON) else {
        return IrRankIndex::default();
    };
    let limit = value
        .get("limit")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .unwrap_or(100);
    let mut charts = HashMap::new();
    if let Some(object) = value.get("charts").and_then(Value::as_object) {
        for (md5, item) in object {
            let md5 = valid_hex(md5.to_string(), 32);
            let Some(values) = item.as_array() else {
                continue;
            };
            let Some(total_players) = values.first().and_then(Value::as_i64) else {
                continue;
            };
            let scores = values
                .iter()
                .skip(1)
                .filter_map(Value::as_i64)
                .filter(|value| *value >= 0)
                .collect::<Vec<_>>();
            if md5.is_empty() || scores.is_empty() {
                continue;
            }
            charts.insert(
                md5,
                IrRankEntry {
                    total_players,
                    scores,
                },
            );
        }
    }
    IrRankIndex { limit, charts }
}

fn embedded_ir_rank(md5: &str, ex_score: Option<i64>) -> Value {
    let md5 = valid_hex(md5.to_string(), 32);
    let Some(entry) = embedded_ir_rank_index().charts.get(&md5) else {
        return Value::Null;
    };
    let total_players = entry
        .total_players
        .max(i64::try_from(entry.scores.len()).unwrap_or(0));
    let Some(ex_score) = ex_score else {
        return json!({
            "rank": Value::Null,
            "totalPlayers": total_players,
            "rankLimit": embedded_ir_rank_index().limit,
            "outOfRange": false,
            "source": "lr2ir-archive-top100",
        });
    };
    if let Some(index) = entry.scores.iter().position(|score| ex_score >= *score) {
        let rank = i64::try_from(index + 1).unwrap_or(1);
        let top_percent = if total_players > 0 {
            Some(round2(rank as f64 / total_players as f64 * 100.0))
        } else {
            None
        };
        return json!({
            "rank": rank,
            "totalPlayers": total_players,
            "topPercent": top_percent,
            "rankLimit": embedded_ir_rank_index().limit,
            "outOfRange": false,
            "estimated": true,
            "source": "lr2ir-archive-top100",
        });
    }
    json!({
        "rank": Value::Null,
        "totalPlayers": total_players,
        "rankLimit": embedded_ir_rank_index().limit,
        "outOfRange": true,
        "thresholdScore": entry.scores.last().copied(),
        "estimated": true,
        "source": "lr2ir-archive-top100",
    })
}

fn enrich_table(mut table: Value, player: &PlayerData, rival: &RivalData) -> Value {
    let charts = table
        .get_mut("charts")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
        .unwrap_or_default();
    let enriched: Vec<Value> = charts
        .into_iter()
        .map(|mut chart| {
            let md5 = valid_hex(first_text(&chart, &["md5", "hintedBmsMd5"]), 32);
            let local_exists = if player.has_song_catalog && !md5.is_empty() {
                Some(player.local_song_hashes.contains(&md5))
            } else {
                None
            };
            let score = player.by_hash.get(&md5);
            let object = chart.as_object_mut().expect("chart must be an object");
            let status = if local_exists == Some(false) {
                "NO SONG".to_string()
            } else {
                score
                    .map(|entry| entry.lamp_status.clone())
                    .unwrap_or_else(|| "NO PLAY".into())
            };
            object.insert("bmsId".into(), Value::Null);
            object.insert(
                "localSongExists".into(),
                local_exists.map(Value::Bool).unwrap_or(Value::Null),
            );
            object.insert("lampStatus".into(), json!(status));
            object.insert(
                "statusDetail".into(),
                json!(if local_exists == Some(false) {
                    "No local song.db song entry"
                } else if score.is_none() {
                    "No local score.db entry"
                } else {
                    ""
                }),
            );
            for (key, value) in [
                (
                    "playCount",
                    score
                        .and_then(|entry| entry.play_count)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
                (
                    "exScore",
                    score
                        .and_then(|entry| entry.ex_score)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
                (
                    "maxExScore",
                    score
                        .and_then(|entry| entry.max_ex_score)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
                (
                    "scoreRate",
                    score
                        .and_then(|entry| entry.score_rate)
                        .and_then(Number::from_f64)
                        .map(Value::Number)
                        .unwrap_or(Value::Null),
                ),
                (
                    "maxOffset",
                    score
                        .and_then(|entry| entry.max_offset)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
                (
                    "badCount",
                    score
                        .and_then(|entry| entry.bad_count)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
                (
                    "poorCount",
                    score
                        .and_then(|entry| entry.poor_count)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
                (
                    "missCount",
                    score
                        .and_then(|entry| entry.miss_count)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
            ] {
                object.insert(key.into(), value);
            }
            object.insert(
                "playOption".into(),
                score
                    .and_then(|entry| entry.play_option.clone())
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "optionCode".into(),
                score
                    .and_then(|entry| entry.option_code)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "randomSeed".into(),
                score
                    .and_then(|entry| entry.random_seed)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "randomLayout".into(),
                score
                    .and_then(|entry| entry.random_layout.clone())
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "keyMode".into(),
                score
                    .and_then(|entry| entry.key_mode)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "rivalComparison".into(),
                rival_comparison(&md5, score, rival),
            );
            let ir_rank = embedded_ir_rank(&md5, score.and_then(|entry| entry.ex_score));
            object.insert(
                "irRank".into(),
                ir_rank
                    .get("rank")
                    .and_then(Value::as_i64)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "irTotalPlayers".into(),
                ir_rank
                    .get("totalPlayers")
                    .and_then(Value::as_i64)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "irTopPercent".into(),
                ir_rank
                    .get("topPercent")
                    .and_then(Value::as_f64)
                    .and_then(Number::from_f64)
                    .map(Value::Number)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "irRankLimit".into(),
                ir_rank
                    .get("rankLimit")
                    .and_then(Value::as_i64)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "irRankOutOfRange".into(),
                ir_rank
                    .get("outOfRange")
                    .and_then(Value::as_bool)
                    .map(Value::Bool)
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "irRankThresholdScore".into(),
                ir_rank
                    .get("thresholdScore")
                    .and_then(Value::as_i64)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            );
            chart
        })
        .collect();
    let summary = count_lamps(&enriched);
    let levels = build_level_summaries(&enriched);
    let stats = table_stats(&enriched);
    let object = table.as_object_mut().expect("table must be object");
    object.insert("charts".into(), Value::Array(enriched));
    object.insert("summary".into(), summary);
    object.insert("levelSummaries".into(), levels);
    object.insert("stats".into(), stats);
    table
}

fn rival_comparison(md5: &str, score: Option<&ScoreEntry>, rival: &RivalData) -> Value {
    let Some(scores) = rival.by_hash.get(md5) else {
        return Value::Null;
    };
    let mut scores = scores.clone();
    scores.sort_by(|left, right| {
        get_i64(right, "exScore")
            .unwrap_or(-1)
            .cmp(&get_i64(left, "exScore").unwrap_or(-1))
    });
    let Some(best) = scores.first().cloned() else {
        return Value::Null;
    };
    let self_ex = score.and_then(|entry| entry.ex_score);
    let rival_ex = get_i64(&best, "exScore");
    let diff = self_ex.zip(rival_ex).map(|(left, right)| left - right);
    json!({
        "rivalCount": scores.len(), "scores": scores, "bestScore": best, "selfExScore": self_ex,
        "selfLamp": score.map(|entry| entry.lamp_status.as_str()).unwrap_or("NO PLAY"), "scoreDiff": diff,
        "scoreResult": diff.map(|value| if value > 0 { "win" } else if value < 0 { "lose" } else { "draw" }).unwrap_or("unknown"),
        "lampResult": "unknown",
    })
}

fn summarize_embedded_ir_rankings(tables: &[Value]) -> Value {
    let mut indexed = 0usize;
    let mut ranked = 0usize;
    let mut out_of_range = 0usize;
    for chart in tables
        .iter()
        .filter_map(|table| table.get("charts").and_then(Value::as_array))
        .flatten()
    {
        if chart
            .get("irTotalPlayers")
            .and_then(Value::as_i64)
            .is_some_and(|value| value > 0)
        {
            indexed += 1;
        }
        if chart
            .get("irRank")
            .and_then(Value::as_i64)
            .is_some_and(|value| value > 0)
        {
            ranked += 1;
        }
        if chart
            .get("irRankOutOfRange")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            out_of_range += 1;
        }
    }
    json!({
        "source": "lr2ir-archive-top100",
        "rankLimit": embedded_ir_rank_index().limit,
        "indexedCharts": indexed,
        "matchedCharts": ranked,
        "outOfRangeCharts": out_of_range,
        "embeddedCharts": embedded_ir_rank_index().charts.len(),
    })
}

fn count_lamps(charts: &[Value]) -> Value {
    let mut counts = Map::new();
    for status in [
        "MAX",
        "PERFECT",
        "FULL COMBO",
        "EX HARD CLEAR",
        "HARD CLEAR",
        "CLEAR",
        "EASY CLEAR",
        "ASSIST",
        "FAILED",
        "NO PLAY",
        "NO SONG",
        "UNMATCHED",
        "UNSUPPORTED",
    ] {
        counts.insert(status.into(), json!(0));
    }
    for chart in charts {
        let status = chart
            .get("lampStatus")
            .and_then(Value::as_str)
            .unwrap_or("UNMATCHED");
        if let Some(value) = counts.get_mut(status) {
            *value = json!(value.as_u64().unwrap_or(0) + 1);
        }
    }
    Value::Object(counts)
}

fn build_level_summaries(charts: &[Value]) -> Value {
    let mut levels: HashMap<String, Vec<Value>> = HashMap::new();
    for chart in charts {
        levels
            .entry(get_text(chart, "level"))
            .or_default()
            .push(chart.clone());
    }
    let mut rows: Vec<Value> = levels.into_iter().map(|(level, charts)| json!({ "level": level, "chartCount": charts.len(), "summary": count_lamps(&charts), "clearRate": clear_rate(&charts), "playedRate": played_rate(&charts), "charts": charts })).collect();
    rows.sort_by_key(|left| get_text(left, "level"));
    Value::Array(rows)
}

fn table_stats(charts: &[Value]) -> Value {
    json!({ "totalCharts": charts.len(), "clearCount": clear_count(charts), "playedCount": played_count(charts), "matchableCount": matchable_count(charts), "clearRate": clear_rate(charts), "playedRate": played_rate(charts) })
}

fn clear_count(charts: &[Value]) -> usize {
    charts
        .iter()
        .filter(|chart| {
            matches!(
                chart.get("lampStatus").and_then(Value::as_str),
                Some(
                    "MAX"
                        | "PERFECT"
                        | "FULL COMBO"
                        | "EX HARD CLEAR"
                        | "HARD CLEAR"
                        | "CLEAR"
                        | "EASY CLEAR"
                        | "ASSIST"
                )
            )
        })
        .count()
}
fn played_count(charts: &[Value]) -> usize {
    charts
        .iter()
        .filter(|chart| {
            !matches!(
                chart.get("lampStatus").and_then(Value::as_str),
                Some("NO PLAY" | "NO SONG" | "UNMATCHED" | "UNSUPPORTED") | None
            )
        })
        .count()
}
fn matchable_count(charts: &[Value]) -> usize {
    charts
        .iter()
        .filter(|chart| {
            !matches!(
                chart.get("lampStatus").and_then(Value::as_str),
                Some("NO SONG" | "UNMATCHED" | "UNSUPPORTED") | None
            )
        })
        .count()
}
fn clear_rate(charts: &[Value]) -> f64 {
    let total = matchable_count(charts);
    if total == 0 {
        0.0
    } else {
        round2(clear_count(charts) as f64 / total as f64 * 100.0)
    }
}
fn played_rate(charts: &[Value]) -> f64 {
    let total = matchable_count(charts);
    if total == 0 {
        0.0
    } else {
        round2(played_count(charts) as f64 / total as f64 * 100.0)
    }
}

fn overall_summary(tables: &[Value]) -> Value {
    let mut unique = HashMap::new();
    let mut entries = 0;
    for table in tables {
        for chart in table
            .get("charts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            entries += 1;
            unique
                .entry(get_text(chart, "key"))
                .or_insert_with(|| chart.clone());
        }
    }
    let charts: Vec<Value> = unique.into_values().collect();
    json!({ "tableCount": tables.len(), "tableEntryCount": entries, "uniqueChartCount": charts.len(), "summary": count_lamps(&charts), "clearRate": clear_rate(&charts), "playedRate": played_rate(&charts), "clearCount": clear_count(&charts), "playedCount": played_count(&charts), "matchableCount": matchable_count(&charts) })
}

fn build_unlisted(player: &PlayerData, tables: &[Value]) -> Vec<Value> {
    let listed: HashSet<String> = tables
        .iter()
        .flat_map(|table| {
            table
                .get("charts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .map(|chart| get_text(chart, "md5").to_ascii_lowercase())
        .filter(|hash| !hash.is_empty())
        .collect();
    player
        .entries
        .iter()
        .filter(|entry| !listed.contains(&entry.md5))
        .map(|entry| {
            json!({
                "key": format!("unlisted:{}", entry.md5),
                "md5": entry.md5,
                "title": entry.title,
                "artist": entry.artist,
                "level": "",
                "lampStatus": entry.lamp_status,
                "playCount": entry.play_count,
                "exScore": entry.ex_score,
                "maxExScore": entry.max_ex_score,
                "scoreRate": entry.score_rate,
                "maxOffset": entry.max_offset,
                "badCount": entry.bad_count,
                "poorCount": entry.poor_count,
                "missCount": entry.miss_count,
                "playOption": entry.play_option,
                "optionCode": entry.option_code,
                "randomSeed": entry.random_seed,
                "randomLayout": entry.random_layout,
                "keyMode": entry.key_mode,
                "isUnlisted": true,
            })
        })
        .collect()
}

fn sync_file_state(path: &str) -> Value {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            json!({ "path": path, "exists": true, "size": metadata.len(), "mtimeMs": metadata.modified().ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|duration| duration.as_millis() as u64) })
        }
        _ => json!({ "path": path, "exists": false, "size": null, "mtimeMs": null }),
    }
}

async fn file_state(value: &str) -> Value {
    sync_file_state(value)
}
fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grades_keep_normal_and_insane_dan_separate() {
        assert_eq!(
            parse_genoside_grade("GENOSIDE2018 段位認定 五段")
                .unwrap()
                .1,
            "☆5"
        );
        assert_eq!(
            parse_genoside_grade("GENOSIDE2018 段位認定 発狂五段")
                .unwrap()
                .1,
            "★5"
        );
    }

    #[test]
    fn force_score_curve_matches_current_spec() {
        assert_eq!(force_score_coefficient(8.0 / 9.0), 0.9);
        assert_eq!(force_score_coefficient(0.9444), 0.98);
        assert_eq!(force_score_coefficient(1.0), 1.0);
    }

    #[test]
    fn clear_zero_stays_no_play_even_when_play_count_exists() {
        assert_eq!(lamp_status(Some(0), Some(3)), "NO PLAY");
        assert_eq!(lamp_status(Some(1), Some(3)), "FAILED");
    }

    #[test]
    fn beatoraja_clear_types_follow_clear_type_ids() {
        let expected = [
            (0, "NO PLAY"),
            (1, "FAILED"),
            (2, "ASSIST"),
            (3, "ASSIST"),
            (4, "EASY CLEAR"),
            (5, "CLEAR"),
            (6, "HARD CLEAR"),
            (7, "EX HARD CLEAR"),
            (8, "FULL COMBO"),
            (9, "PERFECT"),
            (10, "MAX"),
        ];
        for (clear, lamp) in expected {
            assert_eq!(beatoraja_lamp_status(Some(clear), Some(1)), lamp);
        }
    }

    #[test]
    fn beatoraja_score_combines_early_and_late_judges() {
        let (ex, max, rate, offset) = score_values(Some(900), Some(80), Some(1_000), Some(1));
        assert_eq!(ex, Some(1_880));
        assert_eq!(max, Some(2_000));
        assert_eq!(rate, Some(94.0));
        assert_eq!(offset, Some(120));
    }

    #[test]
    fn beatoraja_option_ids_follow_the_official_random_enum() {
        assert_eq!(
            decode_beatoraja_option(0, Some(7)),
            Some(("NONE".into(), 0))
        );
        assert_eq!(
            decode_beatoraja_option(1, Some(7)),
            Some(("MIRROR".into(), 1))
        );
        assert_eq!(
            decode_beatoraja_option(2, Some(7)),
            Some(("RANDOM".into(), 2))
        );
        assert_eq!(
            decode_beatoraja_option(3, Some(7)),
            Some(("R-RANDOM".into(), 3))
        );
        assert_eq!(
            decode_beatoraja_option(4, Some(7)),
            Some(("S-RANDOM".into(), 4))
        );
        assert_eq!(
            decode_beatoraja_option(6, Some(7)),
            Some(("H-RANDOM".into(), 6))
        );
        assert_eq!(
            decode_beatoraja_option(121, Some(14)),
            Some(("1P MIRROR / 2P RANDOM / FLIP".into(), 1))
        );
    }

    #[test]
    fn beatoraja_random_layout_uses_java_seed_only_for_supported_sp_7key_options() {
        let mut cache = HashMap::new();
        let random = beatoraja_score_option_info(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            Some(1),
            Some(2),
            Some(8_005_733),
            Some(7),
            &mut cache,
        );
        assert_eq!(random.play_option.as_deref(), Some("RANDOM"));
        assert_eq!(random.random_layout.as_deref(), Some("4153726"));

        let rotate = beatoraja_score_option_info(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            Some(1),
            Some(3),
            Some(8_005_733),
            Some(7),
            &mut cache,
        );
        assert_eq!(rotate.play_option.as_deref(), Some("R-RANDOM"));
        assert_eq!(rotate.random_layout.as_deref(), Some("1765432"));

        for (option, key_mode) in [(1, Some(7)), (4, Some(7)), (2, Some(14)), (3, Some(14))] {
            let other = beatoraja_score_option_info(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                Some(1),
                Some(option),
                Some(8_005_733),
                key_mode,
                &mut cache,
            );
            assert_eq!(other.random_layout, None);
        }
    }

    #[test]
    fn beatoraja_missing_random_seed_is_non_fatal() {
        let mut cache = HashMap::new();
        let result = beatoraja_score_option_info(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            Some(1),
            Some(2),
            None,
            Some(7),
            &mut cache,
        );
        assert_eq!(result.play_option.as_deref(), Some("RANDOM"));
        assert_eq!(result.random_layout.as_deref(), Some("UNKNOWN"));

        let rotate = beatoraja_score_option_info(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            Some(1),
            Some(3),
            None,
            Some(7),
            &mut cache,
        );
        assert_eq!(rotate.play_option.as_deref(), Some("R-RANDOM"));
        assert_eq!(rotate.random_layout.as_deref(), Some("UNKNOWN"));
    }

    #[test]
    fn lr2_option_code_decodes_random_and_gauge() {
        assert_eq!(
            decode_lr2_option(21, Some(7)).unwrap(),
            DecodedLr2Option {
                label: "RANDOM / HARD".into(),
                player_one_random: 2,
            }
        );
        assert_eq!(decode_lr2_option(10, Some(7)).unwrap().label, "MIRROR");
        assert_eq!(decode_lr2_option(0, Some(7)).unwrap().label, "NONE");
        assert_eq!(
            decode_lr2_option(1_211, Some(14)).unwrap().label,
            "1P MIRROR / 2P RANDOM / FLIP / HARD"
        );
    }

    #[test]
    fn random_layout_is_limited_to_normal_random_sp_7key() {
        let mut cache = HashMap::new();
        let random = lr2_score_option_info(
            "0123456789abcdef0123456789abcdef",
            Some(1),
            Some(20),
            Some(12_183),
            Some(7),
            &mut cache,
        );
        assert_eq!(random.play_option.as_deref(), Some("RANDOM"));
        assert_eq!(random.random_layout.as_deref(), Some("1743265"));

        for (option, key_mode) in [(10, Some(7)), (30, Some(7)), (20, Some(14))] {
            let other = lr2_score_option_info(
                "0123456789abcdef0123456789abcdef",
                Some(1),
                Some(option),
                Some(12_183),
                key_mode,
                &mut cache,
            );
            assert_eq!(other.random_layout, None);
        }
    }

    #[test]
    fn invalid_random_seed_is_reported_without_failing() {
        let mut cache = HashMap::new();
        let result = lr2_score_option_info(
            "0123456789abcdef0123456789abcdef",
            Some(1),
            Some(20),
            Some(i64::MAX),
            Some(7),
            &mut cache,
        );
        assert_eq!(result.random_layout.as_deref(), Some("UNKNOWN"));
    }

    #[test]
    fn lr2_song_catalog_accepts_openlr2_mode_column() {
        let database = Connection::open_in_memory().unwrap();
        database
            .execute("CREATE TABLE song (hash TEXT, mode INTEGER)", [])
            .unwrap();
        assert_eq!(lr2_song_key_mode_column(&database), "mode AS keymode");
    }

    #[test]
    fn beatoraja_history_groups_days_and_resolves_rotate_layout() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "l2tv-beatoraja-history-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let score_path = root.join("score.db");
        let song_path = root.join("songdata.db");
        let history_path = root.join("scoredatalog.db");
        Connection::open(&score_path).unwrap();

        let song_database = Connection::open(&song_path).unwrap();
        song_database
            .execute_batch(
                "CREATE TABLE song (
                    md5 TEXT, sha256 TEXT, title TEXT, subtitle TEXT, artist TEXT, mode INTEGER
                );",
            )
            .unwrap();
        let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        song_database
            .execute(
                "INSERT INTO song VALUES (?1, ?2, 'Rotate Song', ' [EX]', 'Artist', 7)",
                params!["0123456789abcdef0123456789abcdef", sha256],
            )
            .unwrap();

        let history_database = Connection::open(&history_path).unwrap();
        history_database
            .execute_batch(
                "CREATE TABLE scoredatalog (
                    sha256 TEXT, mode INTEGER, clear INTEGER,
                    epg INTEGER, lpg INTEGER, egr INTEGER, lgr INTEGER,
                    egd INTEGER, lgd INTEGER, ebd INTEGER, lbd INTEGER,
                    epr INTEGER, lpr INTEGER, ems INTEGER, lms INTEGER,
                    notes INTEGER, combo INTEGER, minbp INTEGER,
                    option INTEGER, seed INTEGER, date INTEGER, state INTEGER
                );",
            )
            .unwrap();
        let first = Local
            .with_ymd_and_hms(2026, 2, 3, 20, 15, 0)
            .single()
            .unwrap()
            .timestamp();
        let second = first + 180;
        for timestamp in [first, second] {
            history_database
                .execute(
                    "INSERT INTO scoredatalog VALUES (
                        ?1, 0, 6, 900, 0, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        1000, 900, 20, 3, 8005733, ?2, 0
                    )",
                    params![sha256, timestamp],
                )
                .unwrap();
        }

        let result =
            load_beatoraja_history(&score_path, &song_path, 2026, Some("2026-02-03")).unwrap();
        assert_eq!(result["totalPlays"], 2);
        assert_eq!(result["days"][0]["playCount"], 2);
        assert_eq!(result["days"][0]["chartCount"], 1);
        assert_eq!(result["entries"].as_array().unwrap().len(), 2);
        assert_eq!(result["entries"][0]["title"], "Rotate Song [EX]");
        assert_eq!(result["entries"][0]["playOption"], "R-RANDOM");
        assert_eq!(result["entries"][0]["randomLayout"], "1765432");
        assert_eq!(result["legacyLatestOnly"], false);

        drop(history_database);
        drop(song_database);
        std::fs::remove_dir_all(root).unwrap();
    }
}
