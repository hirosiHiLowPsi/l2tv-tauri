import { cp, mkdir, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.join(root, "public");
const target = path.join(root, "dist", "frontend");
const skippedFiles = new Set([
  "assets/force-rating-badges.png",
  "assets/force-rank-badges/rank_badges.json",
  "assets/force-rank-badges/rank_badges_sheet.png",
]);

if (!target.startsWith(`${root}${path.sep}`)) {
  throw new Error("frontend output escaped the project directory");
}

await rm(target, { recursive: true, force: true });
await mkdir(target, { recursive: true });
await cp(source, target, {
  recursive: true,
  filter(sourcePath) {
    const relative = path.relative(source, sourcePath).split(path.sep).join("/");
    return !skippedFiles.has(relative);
  },
});

console.log(`Prepared Tauri frontend: ${target}`);
