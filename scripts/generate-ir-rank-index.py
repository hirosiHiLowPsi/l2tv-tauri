import argparse
import json
import sqlite3
from datetime import datetime, timezone
from pathlib import Path


TARGET_TAGS = {"stella", "satellite"}


def normalize_md5(value):
    text = str(value or "").strip().lower()
    if len(text) == 32 and all(char in "0123456789abcdef" for char in text):
        return text
    return ""


def load_force_constant_md5s(path):
    data = json.loads(path.read_text(encoding="utf-8"))
    result = set()
    for chart in data.get("charts", []):
        md5 = normalize_md5(chart.get("md5"))
        if md5:
            result.add(md5)
    return result


def chart_is_target(row):
    values = [str(value or "").strip() for value in row]
    lowered = [value.lower() for value in values]
    if any(value in TARGET_TAGS for value in lowered):
        return True
    return any(value.startswith("st") or value.startswith("sl") for value in lowered)


def load_st_sl_md5s(connection):
    columns = ", ".join(f"tag_{index}" for index in range(1, 11))
    rows = connection.execute(f"SELECT md5, {columns} FROM chart")
    result = set()
    for row in rows:
        md5 = normalize_md5(row[0])
        if md5 and chart_is_target(row[1:]):
            result.add(md5)
    return result


def build_index(archive_db, force_constants_path, limit):
    connection = sqlite3.connect(f"file:{archive_db}?mode=ro", uri=True)
    connection.execute("PRAGMA query_only = ON")
    target_md5s = load_force_constant_md5s(force_constants_path)
    target_md5s.update(load_st_sl_md5s(connection))

    charts = {}
    for md5 in sorted(target_md5s):
        scores = [
            int(row[0])
            for row in connection.execute(
                "SELECT score FROM pb WHERE md5 = ?1 AND is_cheated = 0 AND rank <= ?2 ORDER BY rank ASC, score DESC LIMIT ?2",
                (md5, limit),
            )
        ]
        if not scores:
            continue
        total = connection.execute("SELECT play_people FROM chart WHERE md5 = ?1", (md5,)).fetchone()
        total_players = int(total[0]) if total and total[0] is not None else 0
        charts[md5] = [total_players, *scores]

    return {
        "v": 1,
        "limit": limit,
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "target": "insane/overjoy/stella/satellite top score index",
        "format": "charts[md5] = [totalPlayers, top1Score, top2Score, ...]",
        "charts": charts,
    }


def main():
    parser = argparse.ArgumentParser(description="Generate compact LR2IR Archive top score index.")
    parser.add_argument("--archive-db", required=True, type=Path)
    parser.add_argument("--force-constants", default=Path("public/data/force-chart-constants.json"), type=Path)
    parser.add_argument("--output", default=Path("public/data/ir-rank-top100.json"), type=Path)
    parser.add_argument("--limit", default=100, type=int)
    args = parser.parse_args()

    data = build_index(args.archive_db, args.force_constants, args.limit)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(data, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    print(f"wrote {args.output} charts={len(data['charts'])} bytes={args.output.stat().st_size}")


if __name__ == "__main__":
    main()
