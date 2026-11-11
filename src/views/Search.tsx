import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { api, IndexStatus, Row } from "../api";
import { askQuestion, Mode, moveSelection, otherMode } from "../lib/search";

type Ask = { phase: "idle" } | { phase: "running"; question: string } | { phase: "error"; message: string };

export function Search({ initialMode }: { initialMode: Mode }) {
  const [mode, setMode] = useState<Mode>(initialMode);
  const [query, setQuery] = useState("");
  const [rows, setRows] = useState<Row[]>([]);
  const [sel, setSel] = useState(0);
  const [ask, setAsk] = useState<Ask>({ phase: "idle" });
  const [askRows, setAskRows] = useState<Row[] | null>(null);
  const [index, setIndex] = useState<IndexStatus | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  // The window is reused: follow route changes (#search/open, #search/history).
  useEffect(() => {
    setMode(initialMode);
    setQuery("");
    input.current?.focus();
  }, [initialMode]);

  const question = askQuestion(query);

  const runSearch = useCallback(async () => {
    if (question !== null) {
      setRows([]);
      return;
    }
    try {
      setRows(await api.search(mode, query));
    } catch {
      setRows([]);
    }
  }, [mode, query, question]);

  const runRef = useRef(runSearch);
  runRef.current = runSearch;

  useEffect(() => {
    setAskRows(null);
    const t = setTimeout(runSearch, 60);
    return () => clearTimeout(t);
  }, [runSearch]);

  useEffect(() => setSel(0), [mode, query, askRows]);

  // Live updates for open sessions; index progress for history.
  useEffect(() => {
    const un = listen("board-changed", () => {
      if (mode === "open" && askRows === null) runSearch();
    });
    return () => {
      un.then((f) => f());
    };
  }, [mode, runSearch, askRows]);

  useEffect(() => {
    if (mode !== "history") return;
    api.refreshHistory();
    let alive = true;
    let wasBuilding = false;
    const poll = async () => {
      const st = await api.indexStatus();
      if (!alive) return;
      setIndex(st);
      if (wasBuilding && !st.building) runRef.current();
      wasBuilding = st.building;
    };
    poll();
    const t = setInterval(poll, 1000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [mode]);

  useEffect(() => input.current?.focus(), [mode]);

  const startAsk = async (q: string) => {
    setAsk({ phase: "running", question: q });
    setAskRows(null);
    try {
      const result = await api.ask(q);
      setAskRows(result);
      setAsk({ phase: "idle" });
    } catch (e) {
      setAsk({ phase: "error", message: String(e) });
    }
  };

  const cancelAsk = async () => {
    await api.cancelAsk();
    setAsk({ phase: "idle" });
  };

  const shown: Row[] = askRows ?? rows;
  const showAskRow = askRows === null && (question !== null || (mode === "history" && query.trim().length > 0));
  const count = shown.length + (showAskRow ? 1 : 0);

  const activate = async (i: number) => {
    if (i >= shown.length) {
      const q = question ?? query.trim();
      if (q) startAsk(q);
      return;
    }
    await api.goTo(shown[i].session_id);
    getCurrentWindow().close();
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Tab") {
      e.preventDefault();
      setMode(otherMode(mode));
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => moveSelection(s, e.key === "ArrowDown" ? 1 : -1, count));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (count > 0) activate(sel);
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (ask.phase === "running") cancelAsk();
      else getCurrentWindow().close();
    }
  };

  useEffect(() => {
    listRef.current?.querySelector("li.sel")?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const building = mode === "history" && index?.building;

  return (
    <div className="search" onKeyDown={onKey}>
      <div className="search-bar">
        <input
          ref={input}
          type="text"
          autoFocus
          spellCheck={false}
          placeholder={mode === "open" ? "Search open sessions…" : "Search history… (? to ask Claude)"}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="mode" title="⇥ switches">
          <span className={mode === "open" ? "on" : ""}>Open</span>
          <span className={mode === "history" ? "on" : ""}>History</span>
        </div>
      </div>
      <ul className="results" ref={listRef}>
        {shown.map((r, i) => (
          <li key={r.session_id} className={i === sel ? "sel" : ""} onMouseEnter={() => setSel(i)} onClick={() => activate(i)}>
            <span className="glyph">{r.glyph}</span>
            <div className="grow">
              <div className="title">
                <strong>{r.folder}</strong> — {r.title}
              </div>
              {r.reason && <div className="sub">{r.reason}</div>}
              {!r.reason && r.snippet && <div className="sub">{r.snippet}</div>}
            </div>
            <span className="age">
              {r.window ? `▢${r.window} · ` : ""}
              {r.ago}
            </span>
          </li>
        ))}
        {showAskRow && (
          <li className={sel === shown.length ? "sel" : ""} onMouseEnter={() => setSel(shown.length)} onClick={() => activate(shown.length)}>
            <span className="glyph">✨</span>
            <div className="grow">
              <div className="title">Ask Claude{question || query.trim() ? `: “${question ?? query.trim()}”` : ""}</div>
              <div className="sub">Claude reads your history (read-only) to find the session.</div>
            </div>
          </li>
        )}
        {count === 0 && !building && (
          <li className="muted">{mode === "open" ? "No open sessions match." : "No sessions match."}</li>
        )}
      </ul>
      <div className="status">
        {ask.phase === "running" && (
          <>
            <span className="grow">Asking Claude… this can take up to a minute.</span>
            <button onClick={cancelAsk}>Cancel</button>
          </>
        )}
        {ask.phase === "error" && <span className="grow bad">{ask.message}</span>}
        {ask.phase === "idle" && (
          <span className="grow">
            {building ? "Building the history index…" : "↑↓ select · ⏎ go · ⇥ switch mode · esc close"}
          </span>
        )}
        {askRows !== null && <button onClick={() => setAskRows(null)}>Back to results</button>}
      </div>
    </div>
  );
}
