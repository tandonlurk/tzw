// Render every composition (or the ids given as args) to renders/<id>.mp4.
// Usage: npm run render            -> all
//        npm run render -- 05 08   -> ids starting with those prefixes
import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";

const list = JSON.parse(execFileSync("npx", ["remotion", "compositions", "src/index.ts", "--quiet", "--json"], { encoding: "utf8" }));
const want = process.argv.slice(2);
const ids = list.map((c) => c.id).filter((id) => want.length === 0 || want.some((w) => id.startsWith(w)));
mkdirSync("renders", { recursive: true });
for (const id of ids) {
  console.log(`rendering ${id}`);
  execFileSync("npx", ["remotion", "render", "src/index.ts", id, `renders/${id}.mp4`, "--codec=h264", "--crf=16"], { stdio: "inherit" });
}
