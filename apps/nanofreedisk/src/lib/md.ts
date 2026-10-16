// The tiny Markdown subset used by Folder Encyclopedia articles: paragraphs
// (blank-line separated), **bold** and `code`. Everything is HTML-escaped first, so
// the output is safe for {@html}.

const ESC: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ESC[c]);
}

/** Inline formatting of one escaped line: `code` first (its content stays literal). */
function inline(text: string): string {
  return text
    .split(/(`[^`]+`)/g)
    .map((part) =>
      part.startsWith("`") && part.endsWith("`") && part.length > 1
        ? `<code>${part.slice(1, -1)}</code>`
        : part.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>"),
    )
    .join("");
}

/** Article body → `<p>` elements. */
export function renderArticle(body: string): string {
  return escapeHtml(body)
    .split(/\n\s*\n/)
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => `<p>${inline(p.replace(/\s*\n\s*/g, " "))}</p>`)
    .join("");
}
