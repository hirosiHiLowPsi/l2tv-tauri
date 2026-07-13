import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const executable = path.join(root, "src-tauri", "target", "debug", "L2TV.exe");
const marker = path.join(root, "tmp", `e2e-marker-${process.pid}.json`);
const webviewData = path.join(root, "tmp", `e2e-webview-${process.pid}`);
const scoreDb = process.env.L2TV_E2E_SCORE_DB?.trim();
const songDb = process.env.L2TV_E2E_SONG_DB?.trim();

if (!scoreDb || !songDb) {
  throw new Error(
    "Set L2TV_E2E_SCORE_DB and L2TV_E2E_SONG_DB to non-sensitive test database paths.",
  );
}

const child = spawn(executable, [], {
  cwd: root,
  windowsHide: true,
  stdio: ["ignore", "pipe", "pipe"],
  env: {
    ...process.env,
    L2TV_E2E_MARKER: marker,
    L2TV_E2E_SCORE_DB: scoreDb,
    L2TV_E2E_SONG_DB: songDb,
    L2TV_WEBVIEW_DATA_DIR: webviewData,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--disable-gpu --no-sandbox",
  },
});
let childOutput = "";
child.stdout.on("data", (chunk) => { childOutput += String(chunk); });
child.stderr.on("data", (chunk) => { childOutput += String(chunk); });

try {
  const result = await waitForMarker();
  assert.equal(result.ok, true, result.error || "E2E script failed");
  assert.equal(result.readyState, "complete");
  assert.equal(result.bridge, "function");
  assert.equal(result.transferBridge, true);
  assert.equal(result.transferButtonsEnabled, true);
  assert.equal(result.menuOpened, true);
  assert.equal(result.scoreDbExists, true);
  assert.equal(typeof result.name, "string");
  assert.equal(typeof result.id, "string");
  assert.equal(typeof result.triple, "boolean");
  if (process.env.L2TV_E2E_EXPECTED_ID) {
    assert.equal(result.id, process.env.L2TV_E2E_EXPECTED_ID);
  }
  if (process.env.L2TV_E2E_EXPECTED_GRADE) {
    assert.equal(result.grade, process.env.L2TV_E2E_EXPECTED_GRADE);
  }
  if (process.env.L2TV_E2E_EXPECTED_ST) {
    assert.equal(result.st, process.env.L2TV_E2E_EXPECTED_ST);
  }
  console.log(JSON.stringify(result, null, 2));
  console.log("Tauri end-to-end smoke test passed.");
} finally {
  if (child.exitCode == null) {
    child.kill();
    await new Promise((resolve) => child.once("exit", resolve));
  }
  await rm(marker, { force: true });
  await rm(webviewData, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}

async function waitForMarker() {
  let lastError;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    if (child.exitCode != null) {
      throw new Error(`L2TV exited before completing E2E (${child.exitCode})\n${childOutput}`);
    }
    try {
      return JSON.parse(await readFile(marker, "utf8"));
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`E2E marker was not created. ${lastError?.message || ""}\n${childOutput}`);
}
