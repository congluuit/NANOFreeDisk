#!/usr/bin/env node
// Fails when a Fluent message exists in one locale but not the other (PLAN §11:
// EN and VI are mandatory everywhere), or when a message's variables differ.
//
//   node scripts/check-locales.mjs [locales-dir]

import { readdirSync, readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), "..", "locales");
const LOCALES = ["en", "vi"];

/** Parse `key = value` messages (with indented continuation lines) → Map<key, vars>. */
function parse(text) {
  const out = new Map();
  let key = null;
  for (const line of text.split(/\r?\n/)) {
    const m = /^([a-zA-Z][\w-]*)\s*=(.*)$/.exec(line);
    if (m) {
      key = m[1];
      if (out.has(key)) throw new Error(`duplicate key ${key}`);
      out.set(key, new Set());
      collect(out.get(key), m[2]);
    } else if (key && /^\s+\S/.test(line)) {
      collect(out.get(key), line);
    } else {
      key = null;
    }
  }
  return out;
}

function collect(set, text) {
  for (const v of text.matchAll(/\{\s*\$([\w-]+)/g)) set.add(v[1]);
}

let errors = 0;
const files = new Set();
for (const loc of LOCALES) {
  const dir = join(root, loc);
  if (!existsSync(dir)) {
    console.error(`missing locale directory ${dir}`);
    errors++;
    continue;
  }
  for (const f of readdirSync(dir)) if (f.endsWith(".ftl")) files.add(f);
}

for (const file of files) {
  const maps = {};
  for (const loc of LOCALES) {
    const path = join(root, loc, file);
    if (!existsSync(path)) {
      console.error(`${loc}/${file}: file missing`);
      errors++;
      continue;
    }
    try {
      maps[loc] = parse(readFileSync(path, "utf8"));
    } catch (e) {
      console.error(`${loc}/${file}: ${e.message}`);
      errors++;
    }
  }
  const [a, b] = LOCALES;
  if (!maps[a] || !maps[b]) continue;
  for (const [x, y] of [
    [a, b],
    [b, a],
  ]) {
    for (const [key, vars] of maps[x]) {
      if (!maps[y].has(key)) {
        console.error(`${file}: "${key}" exists in ${x} but not in ${y}`);
        errors++;
      } else if (x === a) {
        const other = maps[y].get(key);
        const diff = [...vars].filter((v) => !other.has(v)).concat([...other].filter((v) => !vars.has(v)));
        if (diff.length) {
          console.error(`${file}: "${key}" variables differ between ${a} and ${b}: ${diff.join(", ")}`);
          errors++;
        }
      }
    }
  }
  console.log(`${file}: ${maps[a].size} messages in ${LOCALES.join(" + ")}`);
}

// Every literal key the desktop UI asks for — t("key") or a menu/nav `label: "key"` —
// must exist (dynamic keys such as `cat-${c}` are covered by the parity check above).
const ui = join(dirname(fileURLToPath(import.meta.url)), "..", "apps", "nanofreedisk", "src");
const en = existsSync(join(root, "en", "app.ftl")) ? parse(readFileSync(join(root, "en", "app.ftl"), "utf8")) : null;
if (en && existsSync(ui)) {
  const used = new Map();
  const walk = (dir) => {
    for (const f of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, f.name);
      if (f.isDirectory()) walk(path);
      else if (/\.(svelte|ts)$/.test(f.name) && !f.name.endsWith(".test.ts")) {
        const text = readFileSync(path, "utf8");
        for (const m of text.matchAll(/\bt\(\s*"([a-z][\w-]*)"|\blabel:\s*"([a-z]+-[\w-]+)"/g)) {
          const key = m[1] ?? m[2];
          if (!used.has(key)) used.set(key, path);
        }
      }
    }
  };
  walk(ui);
  for (const [key, path] of used) {
    if (!en.has(key)) {
      console.error(`${path}: uses "${key}", which is not in en/app.ftl`);
      errors++;
    }
  }
  console.log(`desktop UI: ${used.size} literal keys, all defined`.replace(", all defined", errors ? "" : ", all defined"));
}

if (errors) {
  console.error(`${errors} locale problem(s)`);
  process.exit(1);
}
