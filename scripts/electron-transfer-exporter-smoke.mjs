import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { access, readFile, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const executable = process.env.L2TV_EXPORTER_EXE?.trim()
  ? path.resolve(process.env.L2TV_EXPORTER_EXE.trim())
  : path.join(root, "src-tauri", "target", "debug", "L2TV.exe");
const executableArguments = process.env.L2TV_EXPORTER_FILENAME_MODE === "1"
  ? []
  : ["--electron-data-exporter"];
const source = process.env.L2TV_EXPORTER_SOURCE_DIR?.trim();
const output = path.join(root, "tmp", `electron-transfer-${process.pid}.json`);

if (!source) {
  throw new Error("Set L2TV_EXPORTER_SOURCE_DIR to an Electron L2TV fixture directory.");
}
await access(executable);
await rm(output, { force: true });

const child = spawn(executable, executableArguments, {
  cwd: root,
  windowsHide: true,
  stdio: ["ignore", "pipe", "pipe"],
  env: {
    ...process.env,
    L2TV_EXPORTER_SOURCE_DIR: source,
    L2TV_EXPORTER_OUTPUT: output,
    L2TV_EXPORTER_AUTO_EXIT: "1",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--disable-gpu --no-sandbox",
  },
});

let childOutput = "";
child.stdout.on("data", (chunk) => { childOutput += String(chunk); });
child.stderr.on("data", (chunk) => { childOutput += String(chunk); });

try {
  const payload = await waitForPayload();
  assert.equal(payload.format, "l2tv-data-transfer");
  assert.equal(payload.version, 1);
  assert.equal(payload.data?.["form-state"]?.themeMode, "dark");
  assert.equal(payload.data?.["last-analysis"]?.player?.name, "Electron Fixture");
  assert.deepEqual(payload.data?.["stellaverse-rival-ids"], ["187038"]);
  console.log("Electron IndexedDB to Tauri transfer JSON smoke test passed.");
} finally {
  if (child.exitCode == null) {
    child.kill();
    await new Promise((resolve) => child.once("exit", resolve));
  }
  await rm(output, { force: true });
}

async function waitForPayload() {
  let lastError;
  for (let attempt = 0; attempt < 180; attempt += 1) {
    try {
      return JSON.parse(await readFile(output, "utf8"));
    } catch (error) {
      lastError = error;
    }
    if (child.exitCode != null) {
      throw new Error(`Exporter exited before writing JSON (${child.exitCode}).\n${childOutput}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Exporter did not create JSON. ${lastError?.message || ""}\n${childOutput}`);
}
