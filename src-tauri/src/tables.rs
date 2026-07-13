use std::collections::HashSet;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use url::Url;

use crate::api::{AppState, CacheEntry, trim_cache};
use crate::error::{Result, message};
use crate::security::{fetch_public_text, normalize_remote_url};

const TABLE_LIST_URL: &str = "https://script.google.com/macros/s/AKfycbzaQbcI9UZDcDlSHHl2NHilhmePrNrwxRdOFkmIXsfnbfksKKmAB3V65WZ8jPWU-7E/exec?table=tablelist";

pub async fn table_list(body: Value, state: &AppState) -> Result<Value> {
    let force = body.get("force").and_then(Value::as_bool).unwrap_or(false);
    if !force {
        let cache = state.table_cache.lock().await;
        if let Some(entry) = cache.get("table-list")
            && entry.fetched_at.elapsed() < Duration::from_secs(3600)
        {
            return Ok(entry.value.clone());
        }
    }
    let response = fetch_public_text(TABLE_LIST_URL, state).await?;
    let parsed = parse_table_list(&response.text, &response.final_url);
    let mut seen = HashSet::new();
    let mut tables: Vec<Value> = parsed
        .into_iter()
        .filter(|entry| {
            entry
                .get("tag1")
                .and_then(Value::as_str)
                .is_some_and(is_sp_tag)
        })
        .filter(|entry| {
            seen.insert(
                entry
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    tables.sort_by(|left, right| {
        let left_year = left
            .get("year")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        let right_year = right
            .get("year")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        right_year
            .cmp(&left_year)
            .then_with(|| text(left, "name").cmp(&text(right, "name")))
    });
    let payload = json!({ "sourceUrl": TABLE_LIST_URL, "fetchedAt": chrono::Utc::now().to_rfc3339(), "tables": tables });
    let mut cache = state.table_cache.lock().await;
    cache.insert(
        "table-list".into(),
        CacheEntry {
            fetched_at: Instant::now(),
            value: payload.clone(),
        },
    );
    trim_cache(&mut cache, 300);
    Ok(payload)
}

pub async fn table_meta(body: Value, state: &AppState) -> Result<Value> {
    let url = body.get("url").and_then(Value::as_str).unwrap_or_default();
    let header = load_header(url, state).await?;
    let data_url = header.final_url.join(
        header
            .json
            .get("data_url")
            .and_then(Value::as_str)
            .ok_or_else(|| message("header.json に data_url がありません。"))?,
    )?;
    Ok(json!({
        "id": stable_id(header.source_url.as_str()),
        "name": text(&header.json, "name"),
        "symbol": text(&header.json, "symbol"),
        "url": header.source_url,
        "sourceUrl": header.source_url,
        "headerUrl": header.final_url,
        "dataUrl": data_url,
    }))
}

pub async fn load_table(raw_url: &str, state: &AppState) -> Result<Value> {
    let safe = normalize_remote_url(raw_url)?.to_string();
    {
        let cache = state.table_cache.lock().await;
        if let Some(entry) = cache.get(&safe)
            && entry.fetched_at.elapsed() < Duration::from_secs(1800)
        {
            return Ok(entry.value.clone());
        }
    }
    let header = load_header(&safe, state).await?;
    let data_url = header.final_url.join(
        header
            .json
            .get("data_url")
            .and_then(Value::as_str)
            .ok_or_else(|| message("header.json に data_url がありません。"))?,
    )?;
    let data = fetch_public_text(data_url.as_str(), state).await?;
    let rows: Value = serde_json::from_str(&data.text)?;
    let rows = rows
        .as_array()
        .ok_or_else(|| message("難易度表データが配列ではありません。"))?;
    let symbol = text(&header.json, "symbol");
    let mut charts = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if !row.is_object() {
            continue;
        }
        charts.push(normalize_chart(row, index, &symbol));
    }
    let level_order: Vec<String> = header
        .json
        .get("level_order")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(value_text)
        .filter(|value| !value.is_empty())
        .collect();
    let table = json!({
        "id": stable_id(&safe),
        "name": nonempty(text(&header.json, "name"), "名称不明の難易度表"),
        "symbol": symbol,
        "sourceUrl": safe,
        "headerUrl": header.final_url,
        "dataUrl": data.final_url,
        "levelOrder": level_order,
        "chartCount": charts.len(),
        "charts": charts,
    });
    let mut cache = state.table_cache.lock().await;
    cache.insert(
        safe,
        CacheEntry {
            fetched_at: Instant::now(),
            value: table.clone(),
        },
    );
    trim_cache(&mut cache, 300);
    Ok(table)
}

struct HeaderData {
    source_url: Url,
    final_url: Url,
    json: Value,
}

async fn load_header(raw_url: &str, state: &AppState) -> Result<HeaderData> {
    let source_url = normalize_remote_url(raw_url)?;
    let source = fetch_public_text(source_url.as_str(), state).await?;
    let (final_url, json_text) = if source.text.trim_start().starts_with('{') {
        (source.final_url.clone(), source.text)
    } else {
        let pattern = Regex::new(r#"(?is)<meta[^>]+name\s*=\s*[\"']bmstable[\"'][^>]+content\s*=\s*[\"']([^\"']+)[\"']|<meta[^>]+content\s*=\s*[\"']([^\"']+)[\"'][^>]+name\s*=\s*[\"']bmstable[\"']"#).unwrap();
        let captures = pattern
            .captures(&source.text)
            .ok_or_else(|| message("bmstableメタタグを見つけられませんでした。"))?;
        let path = captures
            .get(1)
            .or_else(|| captures.get(2))
            .map(|value| value.as_str())
            .ok_or_else(|| message("bmstable URLを読み取れませんでした。"))?;
        let url = source.final_url.join(path)?;
        let header = fetch_public_text(url.as_str(), state).await?;
        (header.final_url, header.text)
    };
    let json: Value = serde_json::from_str(&json_text)?;
    if !json.is_object() || json.get("data_url").and_then(Value::as_str).is_none() {
        return Err(message(
            "URLは table.html または header.json を指定してください。",
        ));
    }
    Ok(HeaderData {
        source_url,
        final_url,
        json,
    })
}

fn normalize_chart(row: &Value, index: usize, symbol: &str) -> Value {
    let md5 = valid_hex(text(row, "md5"), 32);
    let sha256 = valid_hex(text(row, "sha256"), 64);
    let serialized = serde_json::to_string(row).unwrap_or_default();
    let hinted_id = Regex::new(r"(?i)\bbmsid=(\d+)")
        .unwrap()
        .captures(&serialized)
        .and_then(|caps| caps[1].parse::<u64>().ok());
    let hinted_md5 = Regex::new(r"(?i)\bbmsmd5=([0-9a-f]{32})")
        .unwrap()
        .captures(&serialized)
        .map(|caps| caps[1].to_ascii_lowercase());
    let title = nonempty(text(row, "title"), "タイトル不明");
    let artist = match (text(row, "artist"), text(row, "subartist")) {
        (artist, sub) if !artist.is_empty() && !sub.is_empty() => format!("{artist} / {sub}"),
        (artist, _sub) if !artist.is_empty() => artist,
        (_, sub) => sub,
    };
    let level = nonempty(text(row, "level"), "未設定");
    let fallback = stable_id(
        &serde_json::to_string(&(symbol, index, &title, &artist, &level, &md5, &sha256))
            .unwrap_or_default(),
    );
    let key = if !md5.is_empty() {
        format!("md5:{md5}")
    } else if let Some(id) = hinted_id {
        format!("bmsid:{id}")
    } else if !sha256.is_empty() {
        format!("sha256:{sha256}")
    } else {
        format!("fallback:{fallback}")
    };
    json!({
        "key": key, "index": index, "level": level, "title": title, "artist": artist,
        "url": text(row, "url"), "urlDiff": text(row, "url_diff"), "md5": md5, "sha256": sha256,
        "hintedBmsId": hinted_id, "hintedBmsMd5": hinted_md5,
    })
}

fn parse_table_list(source: &str, base: &Url) -> Vec<Value> {
    if let Ok(json) = serde_json::from_str::<Value>(source) {
        let mut rows = Vec::new();
        visit_json(&json, base, &mut rows, "", "", "");
        return rows;
    }
    parse_delimited(source, base)
}

fn visit_json(
    value: &Value,
    base: &Url,
    rows: &mut Vec<Value>,
    inherited_year: &str,
    inherited_tag1: &str,
    inherited_tag2: &str,
) {
    if let Some(array) = value.as_array() {
        for item in array {
            visit_json(
                item,
                base,
                rows,
                inherited_year,
                inherited_tag1,
                inherited_tag2,
            );
        }
        return;
    }
    let Some(object) = value.as_object() else {
        return;
    };
    let year = field(object, &["year", "updated", "date"])
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| inherited_year.into());
    let tag1 = field(
        object,
        &[
            "tag1",
            "tag_1",
            "tags1",
            "type",
            "mode",
            "playMode",
            "play_mode",
            "tag",
        ],
    )
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| inherited_tag1.into());
    let tag2 = field(
        object,
        &["tag2", "tag_2", "tags2", "category", "group", "kind"],
    )
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| inherited_tag2.into());
    if let Some(raw_url) = field(
        object,
        &[
            "url",
            "table_url",
            "tableUrl",
            "header_url",
            "headerUrl",
            "data_url",
            "dataUrl",
            "bmstable",
        ],
    ) && let Ok(url) = base.join(raw_url.trim())
        && matches!(url.scheme(), "http" | "https")
    {
        let url_text = url.to_string();
        let name = field(
            object,
            &[
                "name",
                "title",
                "label",
                "tableName",
                "table_name",
                "symbol",
            ],
        )
        .unwrap_or_else(|| table_name_from_url(&url));
        let symbol = field(object, &["symbol", "mark", "prefix", "id"]).unwrap_or_default();
        let comment = field(
            object,
            &["comment", "description", "memo", "note", "remarks"],
        )
        .unwrap_or_default();
        rows.push(json!({ "id": stable_id(&url_text), "name": name, "url": url_text, "symbol": symbol, "type": tag1, "tag1": tag1, "tag": tag2, "tag2": tag2, "comment": comment, "year": year }));
    }
    for child in object.values() {
        if child.is_object() || child.is_array() {
            visit_json(child, base, rows, &year, &tag1, &tag2);
        }
    }
}

fn field(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = object
            .get(*key)
            .map(value_text)
            .filter(|value| !value.is_empty())
        {
            return Some(value);
        }
    }
    for (name, value) in object {
        let normalized = name
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if keys.iter().any(|key| {
            key.chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase()
                == normalized
        }) {
            let value = value_text(value);
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn parse_delimited(source: &str, base: &Url) -> Vec<Value> {
    let lines: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() < 2 {
        return Vec::new();
    }
    let delimiter = if lines[0].contains('\t') {
        '\t'
    } else if lines[0].contains(',') {
        ','
    } else {
        return Vec::new();
    };
    let headers: Vec<String> = lines[0]
        .split(delimiter)
        .map(|value| {
            value
                .trim()
                .to_ascii_lowercase()
                .replace(['_', '-', ' '], "")
        })
        .collect();
    let mut result = Vec::new();
    for line in &lines[1..] {
        let cells: Vec<&str> = line.split(delimiter).map(str::trim).collect();
        let get = |names: &[&str]| -> String {
            names
                .iter()
                .find_map(|name| {
                    headers
                        .iter()
                        .position(|header| header == name)
                        .and_then(|index| cells.get(index))
                        .copied()
                })
                .unwrap_or_default()
                .to_string()
        };
        let raw_url = get(&["url", "tableurl", "headerurl", "table"]);
        let Ok(url) = base.join(&raw_url) else {
            continue;
        };
        let tag1 = get(&["tag1", "type", "mode", "playmode"]);
        let tag2 = get(&["tag2", "category", "group", "kind"]);
        result.push(json!({ "id": stable_id(url.as_str()), "name": get(&["name", "title", "label"]), "url": url, "symbol": get(&["symbol", "id"]), "type": tag1, "tag1": tag1, "tag": tag2, "tag2": tag2, "comment": get(&["comment", "description"]), "year": get(&["year"]) }));
    }
    result
}

fn is_sp_tag(value: &str) -> bool {
    matches!(
        value
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase()
            .as_str(),
        "sp" | "single" | "singleplay"
    )
}

fn stable_id(value: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())[..12].to_string()
}

fn table_name_from_url(url: &Url) -> String {
    url.host_str().unwrap_or("名称不明の難易度表").to_string()
}
fn valid_hex(value: String, length: usize) -> String {
    let value = value.to_ascii_lowercase();
    if value.len() == length && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        value
    } else {
        String::new()
    }
}
fn text(value: &Value, key: &str) -> String {
    value.get(key).map(value_text).unwrap_or_default()
}
fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.split_whitespace().collect::<Vec<_>>().join(" "),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}
fn nonempty(value: String, fallback: &str) -> String {
    if value.is_empty() {
        fallback.into()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_list_tag_filter_is_sp_only() {
        assert!(is_sp_tag("SP"));
        assert!(is_sp_tag("Single Play"));
        assert!(!is_sp_tag("DP"));
    }
}
