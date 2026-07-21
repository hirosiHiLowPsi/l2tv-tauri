use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde_json::{Value, json};

use crate::api::{AppState, CacheEntry, trim_cache};
use crate::database::{force_score_coefficient, force_tier};
use crate::error::{Result, message};
use crate::security;

const STELLAVERSE_ORIGIN: &str = "https://ir.stellabms.xyz";
const FORCE_CONSTANTS_JSON: &str = include_str!("../../public/data/force-chart-constants.json");
const REQUEST_INTERVAL: Duration = Duration::from_millis(350);
const CACHE_TTL: Duration = Duration::from_secs(30 * 60);
const CACHE_MAX_ENTRIES: usize = 100;
const RIVAL_CODES: &[&str] = &[
    "INSANE1", "OVERJOY", "ST", "SL", "SR", "SO", "SN", "DPSL", "DPST",
];
const RANKING_CODES: &[&str] = &["INSANE1", "OVERJOY", "ST", "SL"];

#[derive(Clone, Debug)]
struct Profile {
    name: String,
    grade_sp: String,
    is_private: bool,
}

#[derive(Clone, Debug)]
struct ClearStatus {
    name: String,
    entries: Vec<Value>,
}

pub async fn fetch_rival(options: Value, state: &AppState) -> Result<Value> {
    let player_id = player_id_from(&options, "Stellaverse Rival ID")?;
    let requested_codes = table_codes_from(&options, RIVAL_CODES);
    if requested_codes.is_empty() {
        return Err(message(
            "Stellaverse IRで比較できる難易度表が読み込まれていません。",
        ));
    }

    let cache_key = format!(
        "rival:{player_id}:{}",
        sorted_codes(&requested_codes).join(",")
    );
    if let Some(value) = read_cache(state, &cache_key, CACHE_TTL).await {
        return Ok(value);
    }

    let profile = fetch_profile_cached(&player_id, state).await.ok();
    let mut player_name = profile
        .as_ref()
        .map(|item| item.name.clone())
        .unwrap_or_default();
    let mut entries_by_hash = HashMap::<String, Value>::new();
    let mut failed_tables = Vec::new();
    let mut effective_codes = requested_codes;
    effective_codes.insert("INSANE1".to_string());
    effective_codes.insert("OVERJOY".to_string());

    for table_code in sorted_codes(&effective_codes) {
        match fetch_clear_status_cached(&player_id, &table_code, state).await {
            Ok(result) => {
                if player_name.is_empty() {
                    player_name = result.name;
                }
                for entry in result.entries {
                    let md5 = entry
                        .get("md5")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let replace = entries_by_hash
                        .get(&md5)
                        .is_none_or(|existing| compare_entries(&entry, existing).is_lt());
                    if replace {
                        entries_by_hash.insert(md5, entry);
                    }
                }
            }
            Err(error) => failed_tables.push(json!({
                "tableCode": table_code,
                "error": error.to_string(),
            })),
        }
    }

    if entries_by_hash.is_empty() {
        let detail = failed_tables
            .iter()
            .filter_map(|item| {
                Some(format!(
                    "{}: {}",
                    item.get("tableCode")?.as_str()?,
                    item.get("error")?.as_str()?
                ))
            })
            .collect::<Vec<_>>()
            .join(" / ");
        return Err(message(if detail.is_empty() {
            "Stellaverse IRからスコアを取得できませんでした。".to_string()
        } else {
            detail
        }));
    }

    let grade_sp = profile
        .as_ref()
        .map(|item| item.grade_sp.as_str())
        .unwrap_or_default();
    let force_rating = calculate_force_rating(&entries_by_hash, grade_sp);
    let entries = entries_by_hash.into_values().collect::<Vec<_>>();
    let value = json!({
        "id": player_id,
        "name": sanitize_name(&player_name, &player_id),
        "source": "stellaverse",
        "gradeSp": grade_sp,
        "isPrivate": profile.as_ref().is_some_and(|item| item.is_private),
        "forceRating": force_rating,
        "scoreCount": entries.len(),
        "entries": entries,
        "failedTables": failed_tables,
    });
    write_cache(state, cache_key, value.clone()).await;
    Ok(value)
}

pub async fn fetch_rankings(options: Value, state: &AppState) -> Result<Value> {
    let player_id = player_id_from(&options, "Stellaverse IR ID")?;
    let table_codes = table_codes_from(&options, RANKING_CODES);
    if table_codes.is_empty() {
        return Ok(json!({ "playerId": player_id, "entries": [], "failedTables": [] }));
    }

    let cache_key = format!(
        "rankings:{player_id}:{}",
        sorted_codes(&table_codes).join(",")
    );
    if let Some(value) = read_cache(state, &cache_key, CACHE_TTL).await {
        return Ok(value);
    }

    let mut entries_by_hash = HashMap::<String, Value>::new();
    let mut failed_tables = Vec::new();
    for table_code in sorted_codes(&table_codes) {
        match fetch_clear_status_cached(&player_id, &table_code, state).await {
            Ok(result) => {
                for entry in result.entries {
                    let has_rank = entry.get("rank").is_some_and(Value::is_number)
                        && entry.get("totalPlayers").is_some_and(Value::is_number);
                    if has_rank {
                        let md5 = entry
                            .get("md5")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        entries_by_hash.insert(md5, entry);
                    }
                }
            }
            Err(error) => failed_tables.push(json!({
                "tableCode": table_code,
                "error": error.to_string(),
            })),
        }
    }

    let value = json!({
        "playerId": player_id,
        "entries": entries_by_hash.into_values().collect::<Vec<_>>(),
        "failedTables": failed_tables,
    });
    write_cache(state, cache_key, value.clone()).await;
    Ok(value)
}

pub(crate) async fn fetch_player_profile(player_id: &str, state: &AppState) -> Result<Value> {
    if player_id.is_empty()
        || player_id.len() > 10
        || !player_id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(message("Stellaverse IR IDが不正です。"));
    }
    let profile = fetch_profile_cached(player_id, state).await?;
    Ok(json!({
        "name": profile.name,
        "gradeSp": profile.grade_sp,
        "isPrivate": profile.is_private,
    }))
}

async fn fetch_profile_cached(player_id: &str, state: &AppState) -> Result<Profile> {
    let cache_key = format!("profile:{player_id}");
    if let Some(value) = read_cache(state, &cache_key, CACHE_TTL).await {
        return Ok(Profile {
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(player_id)
                .to_string(),
            grade_sp: value
                .get("gradeSp")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            is_private: value
                .get("isPrivate")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }
    let html = fetch_stellaverse_page(&format!("/players/{player_id}"), state).await?;
    let profile = parse_profile(&html, player_id)?;
    write_cache(
        state,
        cache_key,
        json!({
            "name": profile.name,
            "gradeSp": profile.grade_sp,
            "isPrivate": profile.is_private,
        }),
    )
    .await;
    Ok(profile)
}

async fn fetch_clear_status_cached(
    player_id: &str,
    table_code: &str,
    state: &AppState,
) -> Result<ClearStatus> {
    let cache_key = format!("clear:{player_id}:{table_code}");
    if let Some(value) = read_cache(state, &cache_key, CACHE_TTL).await {
        return Ok(ClearStatus {
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(player_id)
                .to_string(),
            entries: value
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        });
    }
    let html =
        fetch_stellaverse_page(&format!("/clear-status/{table_code}/{player_id}"), state).await?;
    let result = parse_clear_status(&html, player_id)?;
    write_cache(
        state,
        cache_key,
        json!({ "name": result.name, "entries": result.entries }),
    )
    .await;
    Ok(result)
}

async fn fetch_stellaverse_page(path: &str, state: &AppState) -> Result<String> {
    let _permit = state
        .stellaverse_gate
        .acquire()
        .await
        .map_err(|_| message("Stellaverse IR通信を開始できませんでした。"))?;
    let wait_duration = {
        let guard = state.last_stellaverse_request.lock().await;
        guard
            .as_ref()
            .map(|last| REQUEST_INTERVAL.saturating_sub(last.elapsed()))
            .unwrap_or_default()
    };
    if !wait_duration.is_zero() {
        tokio::time::sleep(wait_duration).await;
    }
    *state.last_stellaverse_request.lock().await = Some(Instant::now());

    let url = format!("{STELLAVERSE_ORIGIN}{path}");
    let remote = security::fetch_public_text(&url, state).await?;
    if remote.final_url.host_str() != Some("ir.stellabms.xyz") {
        return Err(message("Stellaverse IR以外へリダイレクトされました。"));
    }
    Ok(remote.text)
}

fn parse_profile(html: &str, player_id: &str) -> Result<Profile> {
    let document = Html::parse_document(html);
    let h2 = selector("h2")?;
    let h4 = selector("h4")?;
    let body = element_text(document.root_element());
    if contains_case_insensitive(&body, "player not found")
        || body.contains("プレイヤーが見つかりません")
    {
        return Err(message(
            "Stellaverse IRのプレイヤーが見つかりませんでした。",
        ));
    }
    let name = document
        .select(&h2)
        .map(element_text)
        .find(|value| !contains_case_insensitive(value, "STELLAVERSE IR"))
        .unwrap_or_else(|| player_id.to_string());
    let grade_heading = document
        .select(&h4)
        .map(element_text)
        .find(|value| contains_case_insensitive(value, "LR2IR ID"))
        .unwrap_or_default();
    let grade_text = Regex::new(r"(?i)\s*-\s*LR2IR ID[\s\S]*$")
        .expect("grade regex")
        .replace(&grade_heading, "")
        .trim()
        .to_string();
    let grade_sp = grade_text
        .split('/')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let is_private = [
        "プロフィールは非公開",
        "プロフィールが非公開",
        "非公開プロフィール",
        "profile is private",
        "private profile",
    ]
    .iter()
    .any(|needle| contains_case_insensitive(&body, needle));
    Ok(Profile {
        name: sanitize_name(&name, player_id),
        grade_sp,
        is_private,
    })
}

fn parse_clear_status(html: &str, player_id: &str) -> Result<ClearStatus> {
    let document = Html::parse_document(html);
    let body = element_text(document.root_element());
    for error_message in [
        "プレイヤーが見つかりません",
        "クリア状況は非公開",
        "Player not found",
        "clear status is private",
    ] {
        if contains_case_insensitive(&body, error_message) {
            return Err(message(error_message));
        }
    }

    parse_clear_status_document(&document, player_id).or_else(|primary_error| {
        if let Some(embedded_html) = extract_embedded_clear_status_html(html) {
            let embedded_document = Html::parse_document(&embedded_html);
            parse_clear_status_document(&embedded_document, player_id)
        } else {
            Err(primary_error)
        }
    })
}

fn parse_clear_status_document(document: &Html, player_id: &str) -> Result<ClearStatus> {
    let table_selector = selector("table")?;
    let header_selector = selector("th")?;
    // Browser DOM inserts <tbody> automatically, but the server-rendered source may
    // contain rows directly under <table>. Read every row and skip header rows.
    let row_selector = selector("tr")?;
    let cell_selector = selector("td")?;
    let link_selector = selector("a[href^=\"/charts/\"]")?;
    let heading_selector = selector("h3")?;
    let md5_regex = Regex::new(r"(?i)/charts/([0-9a-f]{32})").expect("MD5 regex");

    let score_table = document
        .select(&table_selector)
        .find(|table| {
            let headers = table
                .select(&header_selector)
                .map(element_text)
                .collect::<Vec<_>>();
            headers
                .iter()
                .any(|value| normalize_header_label(value).starts_with("EX"))
                && headers
                    .iter()
                    .any(|value| normalize_header_label(value).starts_with("BP"))
        })
        .ok_or_else(|| message("Stellaverse IRのスコア表を確認できませんでした。"))?;

    let heading = document
        .select(&heading_selector)
        .map(element_text)
        .find(|value| {
            value.contains("クリア状況") || contains_case_insensitive(value, "clear status")
        })
        .unwrap_or_default();
    let name_regex = Regex::new(r"(?i)(?:クリア状況|clear status)\s*[—-]\s*(.+?)(?:\s*\(|$)")
        .expect("name regex");
    let name = name_regex
        .captures(&heading)
        .and_then(|capture| capture.get(1))
        .map(|value| sanitize_name(value.as_str(), player_id))
        .unwrap_or_else(|| player_id.to_string());

    let mut entries = Vec::new();
    for row in score_table.select(&row_selector) {
        let cells = row.select(&cell_selector).collect::<Vec<_>>();
        if cells.len() < 6 {
            continue;
        }
        let Some(link) = row.select(&link_selector).next() else {
            continue;
        };
        let href = link.value().attr("href").unwrap_or_default();
        let Some(md5) = md5_regex
            .captures(href)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().to_ascii_lowercase())
        else {
            continue;
        };
        entries.push(normalize_row(&md5, &cells));
    }

    Ok(ClearStatus { name, entries })
}

fn normalize_header_label(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '%')
        .collect::<String>()
        .to_ascii_uppercase()
}

fn extract_embedded_clear_status_html(html: &str) -> Option<String> {
    let mut fragments = Vec::new();
    let script_regex = Regex::new(r#"(?s)self\.__next_f\.push\(\[1,\s*"((?:\\.|[^"\\])*)"\s*\]\)"#)
        .expect("Next flight script regex");
    for capture in script_regex.captures_iter(html) {
        let Some(raw) = capture.get(1).map(|value| value.as_str()) else {
            continue;
        };
        let decoded = decode_javascript_string(raw);
        if decoded.contains("clear-status-table") || decoded.contains("/charts/") {
            fragments.push(decoded);
        }
    }

    let joined = fragments.join("");
    if joined.contains("clear-status-table") || joined.contains("/charts/") {
        Some(joined)
    } else {
        None
    }
}

fn decode_javascript_string(value: &str) -> String {
    let wrapped = format!("\"{}\"", value);
    serde_json::from_str::<String>(&wrapped).unwrap_or_else(|_| {
        value
            .replace("\\\"", "\"")
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\/", "/")
            .replace("\\\\", "\\")
    })
}

fn normalize_row(md5: &str, cells: &[ElementRef<'_>]) -> Value {
    let lamp_class = cells[1].value().attr("class").unwrap_or_default();
    let rank_text = element_text(cells[2]);
    let ex_text = element_text(cells[3]);
    let rate_text = element_text(cells[4]);
    let bp_text = element_text(cells[5]);
    let pair_regex = Regex::new(r"([\d,]+)\s*/\s*([\d,]+)").expect("pair regex");
    let percent_regex = Regex::new(r"([\d.]+)\s*%").expect("percent regex");

    let (rank, total_players) = parse_pair(&rank_text, &pair_regex);
    let (ex_score, max_ex_score) = parse_pair(&ex_text, &pair_regex);
    let score_rate = percent_regex
        .captures(&rate_text)
        .and_then(|capture| capture.get(1))
        .and_then(|value| value.as_str().parse::<f64>().ok())
        .or_else(|| match (ex_score, max_ex_score) {
            (Some(ex), Some(max)) if max > 0 => Some(ex as f64 / max as f64 * 100.0),
            _ => None,
        })
        .map(round_two);
    let top_percent = match (rank, total_players) {
        (Some(rank), Some(total)) if total > 0 => {
            Some(round_two(rank as f64 / total as f64 * 100.0))
        }
        _ => None,
    };
    json!({
        "md5": md5,
        "lampStatus": parse_lamp(lamp_class),
        "exScore": ex_score,
        "maxExScore": max_ex_score,
        "scoreRate": score_rate,
        "missCount": parse_integer(&bp_text),
        "rank": rank,
        "totalPlayers": total_players,
        "topPercent": top_percent,
    })
}

fn calculate_force_rating(entries: &HashMap<String, Value>, grade_sp: &str) -> Value {
    let payload: Value = serde_json::from_str(FORCE_CONSTANTS_JSON).unwrap_or(Value::Null);
    let mut candidates = payload
        .get("charts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|chart| {
            let md5 = chart.get("md5")?.as_str()?.trim().to_ascii_lowercase();
            let constant = chart.get("chartConstant")?.as_f64()?;
            let score = entries.get(&md5)?;
            let ex_score = score.get("exScore")?.as_i64()?;
            let max_ex_score = score.get("maxExScore")?.as_i64()?;
            if max_ex_score <= 0 {
                return None;
            }
            Some(constant * force_score_coefficient(ex_score as f64 / max_ex_score as f64))
        })
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.total_cmp(left));
    let played_charts = candidates.len();
    candidates.truncate(50);
    let dan_constant = dan_constant(grade_sp);
    let total = candidates.iter().sum::<f64>() + dan_constant;
    let denominator = if dan_constant > 0.0 { 51.0 } else { 50.0 };
    let rating = (total / denominator).clamp(0.0, 30.0);
    let (title, tier) = force_tier(rating);
    json!({
        "available": true,
        "rating": rating,
        "title": title,
        "tier": tier,
        "top50Count": candidates.len(),
        "playedCharts": played_charts,
        "hasDanCandidate": dan_constant > 0.0,
    })
}

fn dan_constant(grade: &str) -> f64 {
    match grade.trim() {
        "★1" => 4.29,
        "★2" => 6.24,
        "★3" => 8.32,
        "★4" => 9.72,
        "★5" => 12.4,
        "★6" => 14.22,
        "★7" => 17.28,
        "★8" => 18.68,
        "★9" => 21.35,
        "★10" => 23.41,
        "★★" => 24.44,
        "(^^)" => 26.81,
        _ => 0.0,
    }
}

fn player_id_from(options: &Value, label: &str) -> Result<String> {
    let player_id = options
        .get("playerId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if player_id.is_empty()
        || player_id.len() > 10
        || !player_id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(message(format!(
            "{label}は10桁以内の数字で入力してください。"
        )));
    }
    Ok(player_id)
}

fn table_codes_from(options: &Value, allowed: &[&str]) -> HashSet<String> {
    let allowed = allowed.iter().copied().collect::<HashSet<_>>();
    options
        .get("tableCodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| allowed.contains(value.as_str()))
        .collect()
}

fn sorted_codes(codes: &HashSet<String>) -> Vec<String> {
    let mut values = codes.iter().cloned().collect::<Vec<_>>();
    values.sort();
    values
}

fn parse_pair(value: &str, regex: &Regex) -> (Option<i64>, Option<i64>) {
    let Some(captures) = regex.captures(value) else {
        return (None, None);
    };
    (
        captures
            .get(1)
            .and_then(|item| parse_integer(item.as_str())),
        captures
            .get(2)
            .and_then(|item| parse_integer(item.as_str())),
    )
}

fn parse_integer(value: &str) -> Option<i64> {
    let normalized = value.replace(',', "").trim().to_string();
    if normalized.is_empty() || !normalized.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    normalized.parse::<i64>().ok().filter(|value| *value >= 0)
}

fn parse_lamp(value: &str) -> &'static str {
    let normalized = value.to_ascii_lowercase();
    if normalized.contains("perfect")
        || normalized.contains("fullcombo")
        || normalized.contains("full-combo")
        || normalized.contains("cb-fc")
    {
        "FULL COMBO"
    } else if normalized.contains("hard") {
        "HARD CLEAR"
    } else if normalized.contains("easy") {
        "EASY CLEAR"
    } else if normalized.contains("fail") {
        "FAILED"
    } else if normalized.contains("noplay") || normalized.contains("no-play") {
        "NO PLAY"
    } else if normalized.contains("norm") || normalized.contains("clear") {
        "CLEAR"
    } else {
        "NO PLAY"
    }
}

fn compare_entries(left: &Value, right: &Value) -> std::cmp::Ordering {
    let left_ex = left.get("exScore").and_then(Value::as_i64).unwrap_or(-1);
    let right_ex = right.get("exScore").and_then(Value::as_i64).unwrap_or(-1);
    if left_ex != right_ex {
        return right_ex.cmp(&left_ex);
    }
    lamp_order(left).cmp(&lamp_order(right))
}

fn lamp_order(entry: &Value) -> usize {
    match entry
        .get("lampStatus")
        .and_then(Value::as_str)
        .unwrap_or("NO PLAY")
    {
        "FULL COMBO" => 0,
        "HARD CLEAR" => 1,
        "CLEAR" => 2,
        "EASY CLEAR" => 3,
        "FAILED" => 4,
        _ => 5,
    }
}

fn sanitize_name(value: &str, fallback: &str) -> String {
    let controls_removed = value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    let private_suffix =
        Regex::new(r"(?i)\s*(?:🔒\u{fe0f}?\s*)?(?:非公開プロフィール|private profile)\s*$")
            .expect("private-profile regex");
    let name = private_suffix
        .replace(&controls_removed, "")
        .trim()
        .chars()
        .take(80)
        .collect::<String>();
    if name.is_empty()
        || Regex::new(r"(?i)^(?:🔒\u{fe0f}?\s*)?(?:非公開プロフィール|private profile)$")
            .expect("private-only regex")
            .is_match(&name)
    {
        fallback.to_string()
    } else {
        name
    }
}

fn element_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn round_two(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn selector(value: &str) -> Result<Selector> {
    Selector::parse(value).map_err(|_| message("HTML解析用セレクタが不正です。"))
}

async fn read_cache(state: &AppState, key: &str, ttl: Duration) -> Option<Value> {
    let mut cache = state.stellaverse_cache.lock().await;
    let fresh = cache
        .get(key)
        .is_some_and(|entry| entry.fetched_at.elapsed() < ttl);
    if fresh {
        cache.get(key).map(|entry| entry.value.clone())
    } else {
        cache.remove(key);
        None
    }
}

async fn write_cache(state: &AppState, key: String, value: Value) {
    let mut cache = state.stellaverse_cache.lock().await;
    cache.insert(
        key,
        CacheEntry {
            fetched_at: Instant::now(),
            value,
        },
    );
    trim_cache(&mut cache, CACHE_MAX_ENTRIES);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_parser_reads_name_grade_and_private_state() {
        let html = r#"
          <html><body>
            <h1>STELLAVERSE IR</h1><h2>毛利S-RAN 🔒 非公開プロフィール</h2>
            <h4>(^^) / st10 - LR2IR ID #218699</h4><p>非公開プロフィール</p>
          </body></html>
        "#;
        let profile = parse_profile(html, "218699").unwrap();
        assert_eq!(profile.name, "毛利S-RAN");
        assert_eq!(profile.grade_sp, "(^^)");
        assert!(profile.is_private);
    }

    #[test]
    fn clear_status_parser_reads_played_and_unplayed_rows() {
        let html = r#"
          <html><body><h3>クリア状況 — テスト (発狂BMS難易度表)</h3>
          <table><thead><tr><th>Lv</th><th>Title</th><th>Rank</th><th>EX</th><th>%</th><th>BP</th></tr></thead>
          <tbody>
            <tr><td>★1</td><td class="clear-status-title-cell cb-fc"><a href="/charts/f0a77e53609afe9607053e612c490a85">Chart</a></td><td>61 / 21,460</td><td>3,270 / 3,320</td><td>98.49% (AAA)</td><td>2</td></tr>
            <tr><td>★1</td><td class="clear-status-title-cell cb-noplay"><a href="/charts/3284c41bf372f18adcd1da097aa7e342">No play</a></td><td></td><td></td><td></td><td></td></tr>
          </tbody></table></body></html>
        "#;
        let result = parse_clear_status(html, "1").unwrap();
        assert_eq!(result.name, "テスト");
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0]["lampStatus"], "FULL COMBO");
        assert_eq!(result.entries[0]["exScore"], 3270);
        assert_eq!(result.entries[0]["rank"], 61);
        assert_eq!(result.entries[1]["lampStatus"], "NO PLAY");
        assert!(result.entries[1]["exScore"].is_null());
    }

    #[test]
    fn clear_status_parser_reads_rows_without_tbody() {
        let html = r#"
          <html><body><h3>クリア状況 — テスト (発狂BMS難易度表)</h3>
          <table class="clear-status-table">
            <tr><th>Lv</th><th>Title</th><th>Rank ↕</th><th>EX ↕</th><th>% ↕</th><th>BP ↕</th></tr>
            <tr><td>★1</td><td class="clear-status-title-cell cb-easy"><a href="/charts/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb">Server source row</a></td><td>12 / 345</td><td>1,234 / 2,000</td><td>61.70% (B)</td><td>45</td></tr>
          </table></body></html>
        "#;
        let result = parse_clear_status(html, "1").unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0]["md5"], "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        assert_eq!(result.entries[0]["lampStatus"], "EASY CLEAR");
        assert_eq!(result.entries[0]["rank"], 12);
        assert_eq!(result.entries[0]["exScore"], 1234);
    }

    #[test]
    fn clear_status_parser_reads_next_stream_embedded_table() {
        let embedded = r#"<h3>クリア状況 — テスト (発狂BMS難易度表)</h3>
          <table class="clear-status-table"><thead><tr><th>Lv</th><th>Title</th><th>Rank ↕</th><th>EX ↕</th><th>% ↕</th><th>BP ↕</th></tr></thead>
          <tbody>
            <tr><td>★1</td><td class="clear-status-title-cell cb-hard"><a href="/charts/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa">Streamed</a></td><td>7 / 100</td><td>2,900 / 3,000</td><td>96.67% (AAA)</td><td>12</td></tr>
          </tbody></table>"#;
        let escaped = serde_json::to_string(embedded).unwrap();
        let html = format!(
            r#"<html><body><p>ページを読み込み中</p><script>self.__next_f.push([1,{escaped}])</script></body></html>"#
        );

        let result = parse_clear_status(&html, "1").unwrap();
        assert_eq!(result.name, "テスト");
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0]["md5"], "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(result.entries[0]["lampStatus"], "HARD CLEAR");
        assert_eq!(result.entries[0]["exScore"], 2900);
    }

    #[test]
    fn code_filter_does_not_accept_arbitrary_values() {
        let options = json!({ "tableCodes": ["st", "SL", "evil", "insane1"] });
        let result = table_codes_from(&options, RANKING_CODES);
        assert_eq!(result.len(), 3);
        assert!(!result.contains("EVIL"));
    }
}
