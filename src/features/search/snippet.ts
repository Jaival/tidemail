/**
 * `SearchHit.snippetHtml` is HTML-escaped text with matches wrapped in `<mark>…</mark>` (no other
 * tags). Split on the mark tags and decode the entities ourselves: the result is rendered as text
 * nodes, never as HTML.
 */
const ENTITIES: Record<string, string> = {
  "&amp;": "&",
  "&lt;": "<",
  "&gt;": ">",
  "&quot;": '"',
  "&#39;": "'",
};

export type SnippetPart = { text: string; match: boolean };

export function parseSnippet(html: string): SnippetPart[] {
  const parts: SnippetPart[] = [];
  let match = false;
  for (const piece of html.split(/(<mark>|<\/mark>)/)) {
    if (piece === "<mark>") match = true;
    else if (piece === "</mark>") match = false;
    else if (piece) parts.push({ text: piece.replace(/&(?:amp|lt|gt|quot|#39);/g, (e) => ENTITIES[e]), match });
  }
  return parts;
}
