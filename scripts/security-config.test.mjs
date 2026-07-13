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

const csp = tauriConfig.app?.security?.csp || "";
assert.match(csp, /default-src 'self'/);
assert.match(csp, /base-uri 'none'/);
assert.match(csp, /object-src 'none'/);
assert.match(csp, /frame-src 'none'/);
assert.doesNotMatch(csp, /'unsafe-eval'/);
assert.equal(tauriConfig.app?.security?.freezePrototype, true);
assert.equal(tauriConfig.identifier, "com.hilowpsi.l2tv.tauri");
assert.equal(tauriConfig.bundle?.active, false);
assert.deepEqual(capability.windows, ["main"]);
assert.deepEqual(
  [...capability.permissions].sort(),
  ["core:default", "core:event:default", "core:window:default"].sort(),
);
assert.doesNotMatch(cargoToml, /tauri-plugin-(shell|fs|http)/);
assert.match(cargoToml, /rusqlite = \{ version = "[^"]+", features = \["bundled"\] \}/);
assert.match(cargoToml, /reqwest = \{[^\n]+rustls-tls/);

console.log("Tauri security configuration test passed.");
