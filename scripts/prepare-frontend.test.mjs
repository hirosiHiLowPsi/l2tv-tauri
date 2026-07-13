import assert from "node:assert/strict";
import { readFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const frontend = path.join(root, "dist", "frontend");
const index = await readFile(path.join(frontend, "index.html"), "utf8");
const bridge = await readFile(path.join(frontend, "tauri-bridge.js"), "utf8");
const app = await readFile(path.join(frontend, "app.js"), "utf8");

assert.match(index, /tauri-bridge\.js/);
assert.match(index, /id="export-transfer-button"/);
assert.match(index, /id="import-transfer-button"/);
assert.match(bridge, /fetch_stellaverse_rival/);
assert.match(bridge, /requestApi/);
assert.match(bridge, /export_data_transfer/);
assert.match(bridge, /import_data_transfer/);
assert.match(app, /window\.lr2irDesktop\.requestApi/);
assert.match(app, /l2tv-data-transfer/);
assert.match(app, /buildDataTransferPayload/);
await assert.rejects(stat(path.join(frontend, "assets", "force-rating-badges.png")));
await assert.rejects(
  stat(path.join(frontend, "assets", "force-rank-badges", "rank_badges_sheet.png")),
);

console.log("Tauri frontend staging test passed.");
