// Render every composition (or the ids given as args) to renders/<id>.mp4. Bundles once into out/.
// Usage: npm run render            -> all
//        npm run render -- 05 08   -> ids starting with those prefixes
import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";

execFileSync("npx", ["remotion", "bundle", "src/index.ts", "--out-dir", "out/bundle"], { stdio: "inherit" });
const all = execFileSync("npx", ["remotion", "compositions", "out/bundle", "--quiet"], { encoding: "utf8" }).trim().split(/\s+/);
const want = process.argv.slice(2);
const ids = all.filter((id) => want.length === 0 || want.some((w) => id.startsWith(w)));
mkdirSync("renders", { recursive: true });
for (const id of ids) {
  console.log(`rendering ${id}`);
  execFileSync("npx", ["remotion", "render", "out/bundle", id, `renders/${id}.mp4`, "--codec=h264", "--crf=16"], { stdio: "inherit" });
}
console.log(`done: ${ids.length} scene(s) in renders/`);
