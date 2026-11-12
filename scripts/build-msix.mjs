// Builds the Microsoft Store package: src-tauri/target/msix/LOKI_<version>.0_x64.msix
//
//   npm run build:msix                 release build of loki.exe, then pack
//   npm run build:msix -- --skip-build  pack the existing src-tauri/target/release/loki.exe
//
// Needs the Windows 10/11 SDK (makeappx.exe, makepri.exe); WINDOWS_SDK_BIN overrides the folder.
// The package is left unsigned on purpose: the Store signs it during certification.
// Build-time only; nothing here ships inside LOKI and nothing here uses the network.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const tauriDir = join(root, "src-tauri");
const outDir = join(tauriDir, "target", "msix");
const layout = join(outDir, "layout");
const skipBuild = process.argv.includes("--skip-build");

function fail(msg) {
  console.error(`build:msix failed: ${msg}`);
  process.exit(1);
}

function run(exe, args, opts = {}) {
  const r = spawnSync(exe, args, { stdio: "inherit", ...opts });
  if (r.status !== 0) fail(`${exe.split(/[\\/]/).pop()} exited with ${r.status ?? r.error}`);
}

// 1. One version everywhere. The Store wants four parts ending in .0.
const conf = JSON.parse(readFileSync(join(tauriDir, "tauri.conf.json"), "utf8"));
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(readFileSync(join(tauriDir, "Cargo.toml"), "utf8"))?.[1];
const version = conf.version;
if (!/^\d+\.\d+\.\d+$/.test(version)) fail(`tauri.conf.json version "${version}" must be x.y.z`);
if (pkg.version !== version || cargoVersion !== version) {
  fail(`versions differ: tauri.conf.json ${version}, package.json ${pkg.version}, Cargo.toml ${cargoVersion}`);
}
const msixVersion = `${version}.0`;

// 2. The manifest must keep the vault outside the package (see the comment in AppxManifest.xml).
const manifest = readFileSync(join(tauriDir, "msix", "AppxManifest.xml"), "utf8");
for (const required of [
  "<desktop6:FileSystemWriteVirtualization>disabled</desktop6:FileSystemWriteVirtualization>",
  '<rescap:Capability Name="unvirtualizedResources" />',
]) {
  if (!manifest.includes(required)) fail(`AppxManifest.xml lost a data-safety line: ${required}`);
}
if (/internetClient|privateNetworkClientServer/.test(manifest.replace(/<!--[\s\S]*?-->/g, ""))) {
  fail("AppxManifest.xml declares a network capability; LOKI must not");
}

// 3. Windows SDK tools: the newest SDK that has makeappx.exe.
function sdkBin() {
  if (process.env.WINDOWS_SDK_BIN) return process.env.WINDOWS_SDK_BIN;
  const base = "C:\\Program Files (x86)\\Windows Kits\\10\\bin";
  const versions = existsSync(base) ? readdirSync(base).filter((d) => /^10\.\d+\.\d+\.\d+$/.test(d)) : [];
  versions.sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  for (const v of versions.reverse()) {
    const dir = join(base, v, "x64");
    if (existsSync(join(dir, "makeappx.exe")) && existsSync(join(dir, "makepri.exe"))) return dir;
  }
  fail("Windows SDK not found (makeappx.exe). Install the Windows 11 SDK or set WINDOWS_SDK_BIN.");
}
const bin = sdkBin();

// 4. Release build of loki.exe (no installers: the MSIX replaces them for the Store).
if (!skipBuild) run("npm run tauri build -- --no-bundle", [], { cwd: root, shell: true });
const exe = join(tauriDir, "target", "release", "loki.exe");
if (!existsSync(exe)) fail(`${exe} not found; run without --skip-build`);

// 5. Package layout: loki.exe, images, manifest, resources.pri.
rmSync(layout, { recursive: true, force: true });
mkdirSync(layout, { recursive: true });
cpSync(exe, join(layout, "loki.exe"));
cpSync(join(tauriDir, "msix", "Assets"), join(layout, "Assets"), { recursive: true });
writeFileSync(join(layout, "AppxManifest.xml"), manifest.replace("{{VERSION}}", msixVersion));

const priConfig = join(outDir, "priconfig.xml");
run(join(bin, "makepri.exe"), ["createconfig", "/cf", priConfig, "/dq", "en-US", "/pv", "10.0.0", "/o"]);
// One resources.pri with every scale: the default config splits scales into resource packs,
// which only a bundle installs, so a single .msix would lose its sharper icons.
writeFileSync(priConfig, readFileSync(priConfig, "utf8").replace(/<packaging>[\s\S]*?<\/packaging>/, ""));
run(join(bin, "makepri.exe"), [
  "new", "/pr", layout, "/cf", priConfig, "/mn", join(layout, "AppxManifest.xml"), "/of", join(layout, "resources.pri"), "/o",
]);

// 6. Pack (makeappx also validates the manifest against the schema).
const msix = join(outDir, `LOKI_${msixVersion}_x64.msix`);
run(join(bin, "makeappx.exe"), ["pack", "/d", layout, "/p", msix, "/o"]);

const sha = createHash("sha256").update(readFileSync(msix)).digest("hex");
console.log(`\nMSIX ready: ${msix}\nSHA-256:    ${sha}\nUnsigned on purpose: upload it in Partner Center; the Store signs it.`);
