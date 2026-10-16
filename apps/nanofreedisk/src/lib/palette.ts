// Ranking for the command palette (Ctrl+K, PLAN §13 G3). Pure so it can be tested
// without a DOM. Matching ignores case and Vietnamese diacritics, so "cai dat" finds
// "Cài đặt".

/** Lower-case, diacritics removed (`đ` → `d`). */
export function fold(s: string): string {
  return s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[đĐ]/g, "d")
    .toLowerCase();
}

const BOUNDARY = /[\s\\/._\-:()]/;

/**
 * How well `query` matches `text`: higher is better, `-1` = no match, `0` for an
 * empty query. Prefix > start of a word > substring > scattered letters in order.
 */
export function fuzzyScore(query: string, text: string): number {
  const q = fold(query).trim();
  if (!q) return 0;
  const t = fold(text);
  const at = t.indexOf(q);
  if (at === 0) return 1000 - Math.min(t.length, 200);
  if (at > 0) return (BOUNDARY.test(t[at - 1]) ? 800 : 600) - Math.min(at, 200);
  if (/\s/.test(q)) {
    // every word somewhere, in any order
    let sum = 0;
    for (const word of q.split(/\s+/)) {
      const s = fuzzyScore(word, t);
      if (s < 0) return -1;
      sum += s;
    }
    return Math.min(500, Math.round(sum / q.split(/\s+/).length / 2));
  }
  let from = 0;
  let first = -1;
  let last = -1;
  let score = 300;
  for (const c of q) {
    const j = t.indexOf(c, from);
    if (j < 0) return -1;
    if (last >= 0) score -= j - last - 1;
    else first = j;
    from = j + 1;
    last = j;
  }
  // letters scattered over a long label are noise, not a match
  if (last - first + 1 > 2 * q.length + 2) return -1;
  return Math.max(1, score);
}

export interface Rankable {
  label: string;
  /** Extra words that also match (other language, synonyms), ranked a bit lower. */
  keywords?: string;
}

/** Items matching `query`, best first (stable for ties), at most `limit`. */
export function rank<T extends Rankable>(items: T[], query: string, limit = 50): T[] {
  if (!query.trim()) return items.slice(0, limit);
  return items
    .map((item, i) => {
      const a = fuzzyScore(query, item.label);
      const k = item.keywords ? fuzzyScore(query, item.keywords) : -1;
      const b = k < 0 ? -1 : Math.max(1, k - 50);
      return { item, i, score: Math.max(a, b) };
    })
    .filter((x) => x.score >= 0)
    .sort((x, y) => y.score - x.score || x.i - y.i)
    .slice(0, limit)
    .map((x) => x.item);
}
