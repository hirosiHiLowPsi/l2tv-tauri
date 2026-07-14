import assert from "node:assert/strict";
import { readFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const frontend = path.join(root, "dist", "frontend");
const index = await readFile(path.join(frontend, "index.html"), "utf8");
const bridge = await readFile(path.join(frontend, "tauri-bridge.js"), "utf8");
const app = await readFile(path.join(frontend, "app.js"), "utf8");
const calendar = await readFile(path.join(frontend, "calendar.html"), "utf8");
const calendarScript = await readFile(path.join(frontend, "calendar.js"), "utf8");

assert.match(index, /tauri-bridge\.js/);
assert.match(index, /id="export-transfer-button"/);
assert.match(index, /id="import-transfer-button"/);
assert.match(index, /name="gameDataMode" value="lr2"/);
assert.match(index, /name="gameDataMode" value="beatoraja"/);
assert.doesNotMatch(index, /id="open-beatoraja-calendar-button"/);
assert.match(index, /id="rival-folder-field"/);
assert.match(bridge, /fetch_stellaverse_rival/);
assert.match(bridge, /requestApi/);
assert.match(bridge, /export_data_transfer/);
assert.match(bridge, /import_data_transfer/);
assert.match(bridge, /open_beatoraja_calendar/);
assert.match(bridge, /beatoraja_calendar_data/);
assert.match(app, /window\.lr2irDesktop\.requestApi/);
assert.match(app, /l2tv-data-transfer/);
assert.match(app, /buildDataTransferPayload/);
assert.match(app, /BEATORAJA_LAMP_OPTIONS/);
assert.match(app, /"MAX",\s*"PERFECT",\s*"FULL COMBO",\s*"EX HARD CLEAR"/);
assert.match(app, /gameDataMode,/);
assert.match(app, /songdata\.db/);
assert.match(app, /"R-RANDOM": "R乱"/);
assert.match(app, /setDatabasePathsForMode/);
assert.match(app, /renderBeatorajaCalendarSection/);
assert.match(app, /loadEmbeddedCalendarDocument/);
assert.match(app, /frame\.srcdoc = calendarDocument/);
assert.match(app, /rivalFolderPath: isBeatoraja \? "" : rivalFolderPath/);
assert.match(app, /column\.key === "rival" && !areRivalFeaturesAvailable\(\)/);
assert.match(calendar, /id="heatmap-grid"/);
assert.match(calendarScript, /getBeatorajaCalendarData/);
assert.match(calendarScript, /embeddedCalendarMode/);
assert.match(calendarScript, /\/api\/beatoraja-history/);
assert.match(calendarScript, /scoredatalog\.db/);
for (let lane = 1; lane <= 7; lane += 1) {
  const keyImage = await stat(path.join(frontend, "assets", "keys", `key_${lane}.png`));
  assert.ok(keyImage.isFile());
  assert.ok(keyImage.size > 0);
}
await assert.rejects(stat(path.join(frontend, "assets", "7key-layout-guide.png")));
await assert.rejects(stat(path.join(frontend, "assets", "force-rating-badges.png")));
await assert.rejects(
  stat(path.join(frontend, "assets", "force-rank-badges", "rank_badges_sheet.png")),
);

console.log("Tauri frontend staging test passed.");
