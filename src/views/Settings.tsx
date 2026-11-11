import { useEffect, useState } from "react";
import { api, Settings, SettingsView as View, ShortcutRow } from "../api";
import { accelFromEvent, displayAccel } from "../lib/keys";

function Recorder({ row, onSet }: { row: ShortcutRow; onSet: (accel: string | null) => Promise<string | null> }) {
  const [listening, setListening] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const stop = () => {
    setListening(false);
    api.pauseShortcuts(false);
  };

  const start = () => {
    setError(null);
    setListening(true);
    api.pauseShortcuts(true);
  };

  const onKey = async (e: React.KeyboardEvent) => {
    if (!listening) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.metaKey) {
      stop();
      return;
    }
    const accel = accelFromEvent(e.nativeEvent);
    if (!accel) return;
    stop();
    setError(await onSet(accel));
  };

  return (
    <div className="row">
      <span className="grow">{row.label}</span>
      <div>
        <button className={`recorder ${listening ? "listening" : ""}`} onClick={start} onKeyDown={onKey} onBlur={() => listening && stop()}>
          {listening ? "Type shortcut…" : row.display || displayAccel(row.accel) || "—"}
        </button>
        {error && <div className="error">{error}</div>}
      </div>
      <button onClick={async () => setError(await onSet(null))} disabled={!row.accel}>
        Clear
      </button>
    </div>
  );
}

export function SettingsView() {
  const [view, setView] = useState<View | null>(null);
  const [tab, setTab] = useState<"shortcuts" | "general">("shortcuts");

  useEffect(() => {
    api.getSettings().then(setView);
  }, []);
  if (!view) return null;

  const update = async (patch: Partial<Settings>) => setView(await api.setGeneral({ ...view.settings, ...patch }));
  const setShortcut = async (id: string, accel: string | null): Promise<string | null> => {
    try {
      setView(await api.setShortcut(id, accel));
      return null;
    } catch (e) {
      return String(e);
    }
  };
  const s = view.settings;
  const toggle = (key: keyof Settings, label: string) => (
    <label className="row">
      <input type="checkbox" checked={Boolean(s[key])} onChange={(e) => update({ [key]: e.target.checked })} />
      <span>{label}</span>
    </label>
  );

  return (
    <div className="page">
      <div className="tabs">
        <button className={tab === "shortcuts" ? "on" : ""} onClick={() => setTab("shortcuts")}>
          Shortcuts
        </button>
        <button className={tab === "general" ? "on" : ""} onClick={() => setTab("general")}>
          General
        </button>
      </div>
      {tab === "shortcuts" && (
        <>
          <p className="muted">Click a shortcut, then press the keys. While recording, Hopback's shortcuts are paused.</p>
          <div className="card">
            {view.shortcuts.map((r) => (
              <Recorder key={r.id} row={r} onSet={(a) => setShortcut(r.id, a)} />
            ))}
          </div>
          <div>
            <button onClick={async () => setView(await api.resetShortcuts())}>Restore defaults</button>
          </div>
        </>
      )}
      {tab === "general" && (
        <>
          <h2>Banners</h2>
          <div className="card">
            <div className="row">{toggle("banners_approve", "When a session waits for an approval")}</div>
            <div className="row">{toggle("banners_question", "When a session asks a question or has a plan ready")}</div>
          </div>
          <h2>Ask Claude</h2>
          <div className="card">
            <div className="row">
              <span>Path to claude</span>
              <input
                className="grow"
                type="text"
                placeholder="Automatic"
                defaultValue={s.claude_path}
                onBlur={(e) => update({ claude_path: e.target.value.trim() })}
              />
            </div>
          </div>
          <h2>App</h2>
          <div className="card">
            <div className="row">{toggle("start_at_login", "Start at login")}</div>
            <div className="row">{toggle("check_updates", "Check for updates")}</div>
            <div className="row">{toggle("install_updates_automatically", "Install updates automatically")}</div>
            <div className="row muted">Version {view.version}</div>
          </div>
        </>
      )}
    </div>
  );
}
