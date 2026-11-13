// Size limit of the Duplicates search: what the field shows (a number in KB or MB),
// what the backend gets (bytes) and what the saved setting holds (KiB).

const KIB = 1024;
const MIB = 1024 * 1024;

/** Default limit (mirror of `nfd_dupes::personal::PERSONAL_MIN_SIZE`, 100 KiB): photos,
 * screenshots and documents are mostly above it, icons and small text below. */
export const DEFAULT_MIN_BYTES = 100 * KIB;

export type SizeUnit = "KB" | "MB";

/** The limit in bytes from the saved setting (KiB). */
export function minBytesFromSetting(kb: number): number {
  if (!Number.isFinite(kb) || kb <= 0) return DEFAULT_MIN_BYTES;
  return Math.max(1, Math.round(kb * KIB));
}

/** The saved setting (KiB) for a limit in bytes. */
export function settingFromBytes(bytes: number): number {
  return bytes / KIB;
}

/** Bytes of `value` in `unit` (at least 1: empty files never count). */
export function toBytes(value: number, unit: SizeUnit): number {
  if (!Number.isFinite(value) || value <= 0) return 1;
  return Math.max(1, Math.round(value * (unit === "MB" ? MIB : KIB)));
}

const round2 = (n: number): number => Math.round(n * 100) / 100;

/** How the field shows `bytes`: MB from 1 MB up, KB below. */
export function splitSize(bytes: number): { value: number; unit: SizeUnit } {
  return bytes >= MIB ? { value: round2(bytes / MIB), unit: "MB" } : { value: round2(bytes / KIB), unit: "KB" };
}

/** `bytes` as a `--min-size` argument of `nfd dupes` (K and M are 1024-based there). */
export function cliSize(bytes: number): string {
  const { value, unit } = splitSize(bytes);
  return `${value}${unit}`;
}

/** Steps offered by "include smaller files", largest first. */
const LADDER = [MIB, 100 * KIB, 10 * KIB];

/** The next lower limit worth trying, or null when the limit is already low. */
export function lowerMinSize(bytes: number): number | null {
  return LADDER.find((step) => step < bytes) ?? null;
}
