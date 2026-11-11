// Pure helpers for the search window.

export type Mode = "open" | "history";

/** "?something" (or "? something") is an Ask query; returns the question. */
export function askQuestion(query: string): string | null {
  const m = /^\s*\?\s*(.*)$/s.exec(query);
  if (!m) return null;
  const q = m[1].trim();
  return q.length > 0 ? q : null;
}

/** Move the selection by `delta`, wrapping around `count` rows. */
export function moveSelection(current: number, delta: number, count: number): number {
  if (count <= 0) return 0;
  return (((current + delta) % count) + count) % count;
}

export function otherMode(mode: Mode): Mode {
  return mode === "open" ? "history" : "open";
}

/** Route from the location hash: "#search/history" → ["search", "history"]. */
export function parseRoute(hash: string): { view: string; arg: string | null } {
  const [view, arg] = hash.replace(/^#\/?/, "").split("/");
  return { view: view || "search", arg: arg ?? null };
}
