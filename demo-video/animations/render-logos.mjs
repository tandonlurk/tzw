// Render every logo concept to ../assets/logos/<id>.jpg (1200x800, Devpost 3:2) and <id>-sq.jpg (1024x1024).
import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";

execFileSync("npx", ["remotion", "bundle", "src/index.ts", "--out-dir", "out/bundle"], { stdio: "inherit" });
const ids = execFileSync("npx", ["remotion", "compositions", "out/bundle", "--quiet"], { encoding: "utf8" })
  .trim().split(/\s+/).filter((id) => id.startsWith("logo-"));
mkdirSync("../assets/logos", { recursive: true });
for (const id of ids) {
  execFileSync("npx", ["remotion", "still", "out/bundle", id, `../assets/logos/${id.replace(/^logo-/, "")}.jpg`, "--image-format=jpeg", "--jpeg-quality=92"], { stdio: "ignore" });
  console.log(`rendered ${id}`);
}
