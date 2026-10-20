// The filter of the command palette.

/**
 * A fuzzy match: the query characters appear in the text in the same order. A lower
 * score is a better match. Null if the text does not match.
 */
export function score(query: string, text: string): number | null {
  const q = query.trim().toLowerCase();
  const t = text.toLowerCase();
  if (!q) return 0;
  const at = t.indexOf(q);
  if (at >= 0) return at === 0 ? -200 : /\W/.test(t[at - 1]!) ? -150 : -100 + at;
  let gaps = 0;
  let last = -1;
  for (const c of q) {
    if (c === " ") continue;
    const i = t.indexOf(c, last + 1);
    if (i < 0) return null;
    if (last >= 0) gaps += i - last - 1;
    last = i;
  }
  return gaps;
}

/** The items that match, best first. Items with the same score keep their order. */
export function filter<T>(items: T[], query: string, text: (item: T) => string): T[] {
  return items
    .map((item, i) => ({ item, i, s: score(query, text(item)) }))
    .filter((r): r is { item: T; i: number; s: number } => r.s !== null)
    .sort((a, b) => a.s - b.s || a.i - b.i)
    .map((r) => r.item);
}
