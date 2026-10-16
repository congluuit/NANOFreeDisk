#!/usr/bin/env node
// Write `latest.json` for the Tauri updater (docs/PHASE8_INSTALLERS.md section 2.5)
// from the updater signatures that build-packages.ps1 copied next to the packages.
//
//   node scripts/latest-json.mjs <dist> <version> <owner/repo> [notes.md]
//
// Platforms: `windows-x86_64` and `windows-x86_64-nsis` -> the per-user NSIS
// package, `windows-x86_64-msi` -> the MSI. Without signatures nothing is written
// (exit 0): a release without the signing key simply has no in-place updates.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [dist, version, repo, notesFile] = process.argv.slice(2);
if (!dist || !version || !repo) {
  console.error("usage: latest-json.mjs <dist> <version> <owner/repo> [notes.md]");
  process.exit(2);
}

const base = `https://github.com/${repo}/releases/download/v${version}`;
const pkg = (file) => {
  const sig = join(dist, `${file}.sig`);
  if (!existsSync(join(dist, file)) || !existsSync(sig)) return null;
  return { signature: readFileSync(sig, "utf8").trim(), url: `${base}/${file}` };
};

const nsis = pkg(`NANOFreeDisk-${version}-x64-setup.exe`);
const msi = pkg(`NANOFreeDisk-${version}-x64.msi`);
if (!nsis && !msi) {
  console.log("no updater signatures: latest.json not written");
  process.exit(0);
}

const platforms = {};
if (nsis) {
  platforms["windows-x86_64"] = nsis;
  platforms["windows-x86_64-nsis"] = nsis;
}
if (msi) platforms["windows-x86_64-msi"] = msi;
if (!platforms["windows-x86_64"]) platforms["windows-x86_64"] = msi;

const notes = notesFile && existsSync(notesFile) ? readFileSync(notesFile, "utf8").trim() : `NANOFreeDisk ${version}`;
const out = { version, notes, pub_date: new Date().toISOString(), platforms };
writeFileSync(join(dist, "latest.json"), JSON.stringify(out, null, 2) + "\n");
console.log(`latest.json: ${Object.keys(platforms).join(", ")}`);
