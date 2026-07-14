import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriConfig = JSON.parse(
  await readFile(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"),
);
const capability = JSON.parse(
  await readFile(path.join(root, "src-tauri", "capabilities", "main.json"), "utf8"),
);
const cargoToml = await readFile(path.join(root, "src-tauri", "Cargo.toml"), "utf8");
const packageScript = await readFile(path.join(root, "scripts", "package-release.ps1"), "utf8");
const exporterSource = await readFile(
  path.join(root, "src-tauri", "src", "electron_transfer_exporter.rs"),
  "utf8",
);

const csp = tauriConfig.app?.security?.csp || "";
assert.match(csp, /default-src 'self'/);
assert.match(csp, /base-uri 'none'/);
assert.match(csp, /object-src 'none'/);
assert.match(csp, /frame-src 'self'/);
assert.doesNotMatch(csp, /frame-src[^;]*(?:https?:|\*)/);
assert.match(csp, /connect-src 'self' ipc: http:\/\/ipc\.localhost/);
assert.doesNotMatch(csp, /'unsafe-eval'/);
assert.equal(tauriConfig.app?.security?.freezePrototype, true);
assert.equal(tauriConfig.identifier, "com.hilowpsi.l2tv.tauri");
assert.equal(tauriConfig.bundle?.active, false);
assert.deepEqual(capability.windows, ["main", "beatoraja-calendar"]);
assert.deepEqual(
  [...capability.permissions].sort(),
  ["core:default", "core:event:default", "core:window:default"].sort(),
);
assert.doesNotMatch(cargoToml, /tauri-plugin-(shell|fs|http)/);
assert.match(cargoToml, /rusqlite = \{ version = "[^"]+", features = \["bundled"\] \}/);
assert.match(cargoToml, /reqwest = \{[^\n]+rustls-tls/);
assert.match(packageScript, /L2TV-Electron-Data-Exporter\.exe/);
assert.match(packageScript, /Electron版データ引継ぎツール\.txt/);
assert.match(exporterSource, /TcpListener::bind\(\(DEFAULT_HOST, origin\.port\)\)/);
assert.match(exporterSource, /follow_links\(false\)/);
assert.doesNotMatch(exporterSource, /https:\/\//);

console.log("Tauri security configuration test passed.");
