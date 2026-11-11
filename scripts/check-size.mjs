// Fails if built JS exceeds the budget from CLAUDE.md: total under 250 KB gzipped,
// initial route (entry chunk plus its static imports) under 120 KB gzipped.
// Run after `npm run build`.
import { readdirSync, readFileSync, existsSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const dist = join(fileURLToPath(import.meta.url), "..", "..", "dist");
const TOTAL_BUDGET = 250 * 1024;
const INITIAL_BUDGET = 120 * 1024;

if (!existsSync(dist)) {
  console.error("dist/ not found. Run `npm run build` first.");
  process.exit(1);
}

const assets = join(dist, "assets");
const gz = (file) => gzipSync(readFileSync(file)).length;
const jsFiles = readdirSync(assets).filter((f) => f.endsWith(".js"));
const sizes = Object.fromEntries(jsFiles.map((f) => [f, gz(join(assets, f))]));
const total = Object.values(sizes).reduce((a, b) => a + b, 0);

// Initial route = scripts and modulepreloads referenced from index.html, plus their static imports.
const html = readFileSync(join(dist, "index.html"), "utf8");
const initial = new Set();
const visit = (name) => {
  if (initial.has(name) || !sizes[name]) return;
  initial.add(name);
  const src = readFileSync(join(assets, name), "utf8");
  for (const m of src.matchAll(/from\s*["']\.\/([^"']+\.js)["']/g)) visit(m[1]);
  for (const m of src.matchAll(/^import\s*["']\.\/([^"']+\.js)["']/gm)) visit(m[1]);
};
for (const m of html.matchAll(/assets\/([^"']+\.js)/g)) visit(m[1]);
const initialSize = [...initial].reduce((a, n) => a + sizes[n], 0);

const kb = (n) => (n / 1024).toFixed(1) + " KB";
console.log(`JS gzipped total:   ${kb(total)} (budget ${kb(TOTAL_BUDGET)})`);
console.log(`JS gzipped initial: ${kb(initialSize)} (budget ${kb(INITIAL_BUDGET)})`);
if (total > TOTAL_BUDGET || initialSize > INITIAL_BUDGET) {
  console.error("npm run size FAILED: over budget");
  process.exit(1);
}
console.log("npm run size passed.");
