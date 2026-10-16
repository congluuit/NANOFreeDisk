#!/usr/bin/env node
// Regenerates laptop-ddr5.bin: a synthetic Windows RawSMBIOSData blob (SMBIOS 3.4)
// shaped like a hybrid-CPU laptop with two DDR5 SODIMMs and one empty slot. No real
// machine's data (serials are fake on purpose and must never be parsed).
//
//   node crates/nfd-hwinfo/tests/fixtures/smbios/make.mjs

import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

function structure(type, handle, fields, strings) {
  const head = Buffer.alloc(4 + fields.length);
  head[0] = type;
  head[1] = head.length;
  head.writeUInt16LE(handle, 2);
  Buffer.from(fields).copy(head, 4);
  const tail = strings.length
    ? Buffer.concat([...strings.map((s) => Buffer.from(s + "\0", "latin1")), Buffer.from([0])])
    : Buffer.from([0, 0]);
  return Buffer.concat([head, tail]);
}
const u16 = (v) => [v & 255, v >> 8];
const u32 = (v) => [v & 255, (v >> 8) & 255, (v >> 16) & 255, (v >>> 24) & 255];
/** Fill a formatted area of `len` bytes (offsets from the structure start). */
function area(len, set) {
  const a = new Array(len - 4).fill(0);
  for (const [off, bytes] of set) bytes.forEach((b, i) => (a[off - 4 + i] = b));
  return a;
}

const parts = [
  // BIOS: vendor 1, version 2, date 3
  structure(0, 0, area(0x1a, [[0x04, [1]], [0x05, [2]], [0x08, [3]]]), ["Acme BIOS", "1.07", "03/14/2025"]),
  // System: manufacturer, product, version, serial (fake), sku, family
  structure(
    1,
    1,
    area(0x1b, [[0x04, [1]], [0x05, [2]], [0x06, [3]], [0x07, [4]], [0x19, [5]], [0x1a, [6]]]),
    ["Acme", "Laptop 15", "To Be Filled By O.E.M.", "SERIAL-MUST-NOT-APPEAR", "AC-15-X", "Workstation"],
  ),
  // Baseboard
  structure(2, 2, area(0x0f, [[0x04, [1]], [0x05, [2]], [0x06, [3]], [0x07, [4]]]), ["Acme", "LP15-MB", "Rev A", "BOARD-SERIAL"]),
  // Chassis: notebook (0x0A)
  structure(3, 3, area(0x15, [[0x04, [1]], [0x05, [0x0a]]]), ["Acme"]),
  // Processor: socket 1, populated (status 0x41), ext 100, max 5400, current 2200
  structure(
    4,
    4,
    area(0x30, [[0x04, [1]], [0x07, [2]], [0x10, [3]], [0x12, u16(100)], [0x14, u16(5400)], [0x16, u16(2200)], [0x18, [0x41]]]),
    ["U3E1", "Intel(R) Corporation", "Intel(R) Core(TM) i9"],
  ),
  // Memory array: system memory (0x03), max 64 GB (in KB), 2 devices
  structure(16, 16, area(0x17, [[0x05, [0x03]], [0x07, u32(64 * 1024 * 1024)], [0x0d, u16(2)]]), []),
  // Memory device: 16 GB SODIMM DDR5 5600 rated / 5200 configured, Samsung by JEDEC id
  structure(
    17,
    17,
    area(0x5c, [[0x0c, u16(16384)], [0x0e, [0x0d]], [0x10, [1]], [0x11, [2]], [0x12, [0x22]], [0x15, u16(5600)], [0x17, [3]], [0x18, [4]], [0x1a, [5]], [0x1b, [1]], [0x20, u16(5200)]]),
    ["DIMM A", "BANK 0", "80CE", "MEM-SERIAL", "M425R2GA3BB0-CWMOD"],
  ),
  // empty slot
  structure(17, 18, area(0x5c, [[0x0c, u16(0)], [0x10, [1]]]), ["DIMM B"]),
  structure(127, 127, [], []),
];
const table = Buffer.concat(parts);
const header = Buffer.from([0, 3, 4, 0, ...u32(table.length)]);
const out = join(dirname(fileURLToPath(import.meta.url)), "laptop-ddr5.bin");
writeFileSync(out, Buffer.concat([header, table]));
console.log(`wrote ${out} (${table.length + 8} bytes)`);
