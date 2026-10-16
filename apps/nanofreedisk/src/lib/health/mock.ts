// Browser-only stand-in for the Health commands (`pnpm dev` without Tauri): an NVMe
// system drive, a SATA hard disk with pending sectors and a USB disk whose bridge
// does not pass SMART through.

import type { Attribute, Drive, HealthReport, HealthVolume } from "./types";

const GB = 2 ** 30;
const now = () => Math.floor(Date.now() / 1000);

const NAMES: Record<number, string> = {
  0x01: "Read Error Rate",
  0x03: "Spin-Up Time",
  0x05: "Reallocated Sectors Count",
  0x09: "Power-On Hours",
  0x0c: "Power Cycle Count",
  0xc2: "Temperature",
  0xc5: "Current Pending Sector Count",
  0xc6: "Uncorrectable Sector Count",
  0xc7: "UltraDMA CRC Error Count",
};

function attr(id: number, current: number, worst: number, raw: number, threshold: number | null): Attribute {
  return { id, flags: id === 5 || id === 1 ? 0x33 : 0x32, current, worst, raw, threshold, name: NAMES[id] ?? null };
}

function drives(elevated: boolean): Drive[] {
  const thr = (t: number) => (elevated ? t : null);
  return [
    {
      index: 0,
      model: "Samsung SSD 980 PRO 1TB",
      serial: "S5P2NS0W000000X",
      firmware: "5B2QGXA7",
      bus: "NVMe",
      size: 1000 * 1e9,
      ssd: true,
      removable: false,
      trim: true,
      status: "good",
      reasons: [],
      temperature: 47,
      tempWarning: 82,
      tempCritical: 85,
      heat: "normal",
      lifeLeft: 96,
      powerOnHours: 4287,
      powerCycles: 1393,
      bytesWritten: 63_710_180 * 512_000,
      bytesRead: 60_884_609 * 512_000,
      unsafeShutdowns: 77,
      source: "nvme",
      note: null,
      nvme: {
        criticalWarning: 0,
        temperature: 47,
        availableSpare: 100,
        spareThreshold: 10,
        percentageUsed: 4,
        dataUnitsRead: 60_884_609,
        dataUnitsWritten: 63_710_180,
        hostReads: 994_100_866,
        hostWrites: 1_214_809_884,
        controllerBusy: 4231,
        powerCycles: 1393,
        powerOnHours: 4287,
        unsafeShutdowns: 77,
        mediaErrors: 0,
        errorLogEntries: 0,
        warningTempMinutes: 0,
        criticalTempMinutes: 0,
        sensors: [47, 55],
      },
      ata: null,
      volumes: ["C:\\"],
    },
    {
      index: 1,
      model: "WDC WD20EZRZ-00Z5HB0",
      serial: "WD-WCC4M0000000",
      firmware: "80.00A80",
      bus: "SATA",
      size: 2000 * 1e9,
      ssd: false,
      removable: false,
      trim: false,
      status: "caution",
      reasons: [{ key: "ata-pending", value: 8, attr: 0xc5 }],
      temperature: 33,
      tempWarning: null,
      tempCritical: null,
      heat: "normal",
      lifeLeft: null,
      powerOnHours: 33_712,
      powerCycles: 1007,
      bytesWritten: null,
      bytesRead: null,
      unsafeShutdowns: null,
      source: elevated ? "ata-smart" : "ata-driver",
      note: elevated ? null : "needs-admin",
      nvme: null,
      ata: {
        thresholds: elevated,
        predictFailure: false,
        attributes: [
          attr(0x01, 200, 200, 0, thr(51)),
          attr(0x03, 172, 170, 4383, thr(21)),
          attr(0x05, 200, 200, 0, thr(140)),
          attr(0x09, 54, 54, 33_712, thr(0)),
          attr(0x0c, 100, 100, 1007, thr(0)),
          attr(0xc2, 114, 101, 33, thr(0)),
          attr(0xc5, 200, 200, 8, thr(0)),
          attr(0xc6, 100, 253, 0, thr(0)),
          attr(0xc7, 200, 200, 0, thr(0)),
        ],
      },
      volumes: ["D:\\"],
    },
    {
      index: 2,
      model: "JMicron Generic",
      serial: null,
      firmware: "0508",
      bus: "USB",
      size: 500 * 1e9,
      ssd: null,
      removable: true,
      trim: null,
      status: "unknown",
      reasons: [],
      temperature: null,
      tempWarning: null,
      tempCritical: null,
      heat: "unknown",
      lifeLeft: null,
      powerOnHours: null,
      powerCycles: null,
      bytesWritten: null,
      bytesRead: null,
      unsafeShutdowns: null,
      source: "none",
      note: "usb-bridge",
      nvme: null,
      ata: null,
      volumes: ["E:\\"],
    },
  ];
}

function volume(root: string, fs: string, total: number, free: number, disk: number, extra: Partial<HealthVolume> = {}): HealthVolume {
  return {
    root,
    label: "",
    fs,
    total,
    free,
    system: root === "C:\\",
    removable: false,
    disk,
    offset: 1 << 20,
    aligned: true,
    cluster: 4096,
    trimEnabled: fs === "NTFS" ? true : null,
    bitlocker: "off",
    dirty: null,
    daysToFull: null,
    hasTrend: false,
    ...extra,
  };
}

let elevated = false;

function report(): HealthReport {
  return {
    schema: "nanofreedisk.health.v1",
    generatedAt: now(),
    elevated,
    drives: drives(elevated),
    volumes: [
      volume("C:\\", "NTFS", 930 * GB, 161 * GB, 0, { daysToFull: 41, hasTrend: true, bitlocker: "on", dirty: elevated ? false : null }),
      volume("D:\\", "NTFS", 1863 * GB, 840 * GB, 1, { hasTrend: true, dirty: elevated ? false : null }),
      volume("E:\\", "exFAT", 465 * GB, 120 * GB, 2, { removable: true, cluster: 131_072, aligned: true, bitlocker: null }),
    ],
  };
}

export const healthHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  health_report: () => report(),
  health_elevated: () => {
    elevated = true;
    return report();
  },
  health_export: () => undefined,
  health_watch_set: () => undefined,
};
