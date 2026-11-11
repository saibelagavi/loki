// LOKI offline guard. Fails (exit 1) if anything in the production build could open a socket.
// Run with: npm run check:offline
//
// Checks:
//  1. Rust: the Windows PRODUCTION dependency graph (cargo tree, normal+build edges) has no
//     networking crates, and tokio is not built with net features. Cargo.lock itself lists
//     crates for other platforms that never compile on Windows, so we ask cargo, not grep.
//  2. npm: production dependency tree has no networking packages or forbidden Tauri plugins.
//  3. Config: tauri.conf.json CSP is the locked-down one; Cargo.toml has no forbidden plugins;
//     capabilities grant no http/shell/updater/fs permissions.
//  4. Source: no http(s):// URL or network API appears in src/ or index.html outside comments.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join, extname, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(import.meta.url), "..", "..");
const failures = [];
const notes = [];
const fail = (msg) => failures.push(msg);

const FORBIDDEN_CRATES = new Set([
  "reqwest", "hyper", "hyper-util", "hyper-tls", "hyper-rustls", "ureq", "curl", "curl-sys",
  "isahc", "attohttpc", "surf", "h2", "h3", "quinn", "native-tls", "openssl", "openssl-sys",
  "rustls", "tungstenite", "tokio-tungstenite", "async-tungstenite", "tower-http",
  "tauri-plugin-http", "tauri-plugin-updater", "tauri-plugin-shell", "tauri-plugin-websocket",
  "sentry", "sentry-core", "minidumper", "crash-handler",
]);
const FORBIDDEN_TOKIO_FEATURES = ["net", "full", "process", "signal"];

const FORBIDDEN_NPM = [
  /^axios$/, /^node-fetch$/, /^got$/, /^ky$/, /^superagent$/, /^undici$/, /^ws$/, /^socket\.io/,
  /^@sentry\//, /^posthog/, /^mixpanel/, /^@vercel\/analytics/, /^react-ga/, /^firebase/,
  /^@tauri-apps\/plugin-(http|updater|shell|websocket|upload|process|os|fs|store|sql)$/,
];

// ---------- 1. Rust production graph ----------
function cargoBin() {
  const local = join(homedir(), ".cargo", "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
  return existsSync(local) ? local : "cargo";
}

function rust() {
  const manifest = join(root, "src-tauri", "Cargo.toml");
  if (!existsSync(manifest)) return fail("src-tauri/Cargo.toml not found");
  let out;
  try {
    out = execFileSync(
      cargoBin(),
      [
        "tree", "--manifest-path", manifest, "-e", "normal,build",
        "--target", "x86_64-pc-windows-msvc", "--prefix", "none",
        "--format", "{p}|{f}", "--locked",
      ],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], maxBuffer: 64 * 1024 * 1024 },
    );
  } catch (e) {
    const first = String(e.stderr || e.message).split("\n")[0];
    return fail(`could not run cargo tree (is Rust installed and Cargo.lock present?): ${first}`);
  }
  const seen = new Set();
  for (const line of out.split("\n")) {
    const [pkg, features = ""] = line.trim().split("|");
    const name = pkg?.split(" ")[0];
    if (!name || seen.has(pkg)) continue;
    seen.add(pkg);
    if (FORBIDDEN_CRATES.has(name)) fail(`Rust production build includes networking crate: ${name}`);
    if (name === "tokio") {
      const bad = features.split(",").filter((f) => FORBIDDEN_TOKIO_FEATURES.includes(f));
      if (bad.length) fail(`tokio is built with socket/process features: ${bad.join(", ")}`);
    }
  }
  notes.push(`Rust: ${seen.size} crates in the Windows production graph checked`);
}

// ---------- 2. npm production tree ----------
function npm() {
  let out;
  try {
    out = execFileSync(
      process.platform === "win32" ? "npm.cmd" : "npm",
      ["ls", "--omit=dev", "--all", "--json"],
      {
        cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"],
        maxBuffer: 64 * 1024 * 1024, shell: process.platform === "win32",
      },
    );
  } catch (e) {
    out = e.stdout; // npm ls exits non-zero on peer warnings but still prints JSON
  }
  if (!out) return fail("could not read npm production dependency tree");
  const names = new Set();
  const walk = (deps) => {
    for (const [name, info] of Object.entries(deps ?? {})) {
      names.add(name);
      walk(info.dependencies);
    }
  };
  walk(JSON.parse(out).dependencies);
  for (const n of names) {
    if (FORBIDDEN_NPM.some((re) => re.test(n))) fail(`npm production tree includes forbidden package: ${n}`);
  }
  notes.push(`npm: ${names.size} production packages checked`);
}

// ---------- 3. Config ----------
const EXPECTED_CSP =
  "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost";

function config() {
  const conf = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
  if (conf.app?.security?.csp !== EXPECTED_CSP) fail("tauri.conf.json: production CSP differs from the one in CLAUDE.md");
  if (conf.plugins?.updater) fail("tauri.conf.json: updater is configured");
  if (conf.bundle?.createUpdaterArtifacts) fail("tauri.conf.json: updater artifacts enabled");

  const cargo = readFileSync(join(root, "src-tauri", "Cargo.toml"), "utf8");
  for (const c of FORBIDDEN_CRATES) {
    if (new RegExp(`^\\s*${c}\\s*=`, "m").test(cargo)) fail(`Cargo.toml declares forbidden crate: ${c}`);
  }

  const capDir = join(root, "src-tauri", "capabilities");
  for (const f of existsSync(capDir) ? readdirSync(capDir) : []) {
    if (!f.endsWith(".json")) continue;
    const cap = JSON.parse(readFileSync(join(capDir, f), "utf8"));
    for (const p of cap.permissions ?? []) {
      const id = typeof p === "string" ? p : p.identifier;
      if (/^(http|shell|updater|websocket|process|os|fs|upload):/.test(id)) {
        fail(`capabilities/${f}: forbidden permission ${id}`);
      }
    }
  }
}

// ---------- 4. Source URLs outside comments ----------
// Tiny scanner so a URL inside a string is seen, and a "//" inside a string is not a comment.
function stripComments(text, ext) {
  const css = ext === ".css";
  if (ext === ".html") return text.replace(/<!--[\s\S]*?-->/g, "");
  let out = "";
  let i = 0;
  let quote = null;
  while (i < text.length) {
    const c = text[i];
    const n = text[i + 1];
    if (quote) {
      out += c;
      if (c === "\\" && !css) {
        out += n ?? "";
        i += 2;
        continue;
      }
      if (c === quote) quote = null;
      i++;
    } else if (c === "/" && n === "*") {
      const end = text.indexOf("*/", i + 2);
      i = end === -1 ? text.length : end + 2;
    } else if (!css && c === "/" && n === "/") {
      while (i < text.length && text[i] !== "\n") i++;
    } else {
      if (c === '"' || c === "'" || (c === "`" && !css)) quote = c;
      out += c;
      i++;
    }
  }
  return out;
}

function walkFiles(dir, acc = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walkFiles(p, acc);
    else if ([".ts", ".tsx", ".js", ".jsx", ".css", ".html"].includes(extname(p))) acc.push(p);
  }
  return acc;
}

function sources() {
  const files = [...walkFiles(join(root, "src")), join(root, "index.html")];
  for (const f of files) {
    const code = stripComments(readFileSync(f, "utf8"), extname(f));
    const rel = relative(root, f);
    if (/https?:\/\//i.test(code)) fail(`${rel}: contains an http(s):// URL outside comments`);
    if (/\b(?:src|href)\s*=\s*["']\/\//i.test(code)) fail(`${rel}: protocol-relative URL`);
    if (/\b(?:fetch|XMLHttpRequest|WebSocket|EventSource|sendBeacon)\b/.test(code)) {
      fail(`${rel}: uses a network API (fetch/XHR/WebSocket/EventSource/sendBeacon)`);
    }
  }
  notes.push(`Source: ${files.length} files scanned`);
}

for (const step of [rust, npm, config, sources]) {
  try {
    step();
  } catch (e) {
    fail(`${step.name}: ${e.message}`);
  }
}
for (const n of notes) console.log(`  ${n}`);
if (failures.length) {
  console.error("\ncheck:offline FAILED");
  for (const f of failures) console.error(`  x ${f}`);
  process.exit(1);
}
console.log("check:offline passed: no network code found in the production build.");
