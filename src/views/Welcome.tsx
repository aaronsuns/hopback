import { useEffect, useState } from "react";
import { api, WelcomeStatus } from "../api";

function Item({ done, title, children }: { done: boolean; title: string; children: React.ReactNode }) {
  return (
    <div className="row" style={{ alignItems: "flex-start" }}>
      <span className={`check ${done ? "ok" : "muted"}`}>{done ? "✓" : "○"}</span>
      <div className="grow">
        <strong>{title}</strong>
        <div>{children}</div>
      </div>
    </div>
  );
}

export function Welcome() {
  const [st, setSt] = useState<WelcomeStatus | null>(null);
  const [banner, setBanner] = useState(false);

  useEffect(() => {
    let alive = true;
    const poll = async () => {
      const s = await api.welcomeStatus();
      if (alive) setSt(s);
    };
    poll();
    const t = setInterval(poll, 1000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);
  if (!st) return null;

  return (
    <div className="page">
      <h1>Welcome to Hopback</h1>
      <p className="muted">A few things make Hopback work. This list ticks itself off as you go.</p>
      <div className="card">
        <Item done={st.accessibility} title="Accessibility">
          <p className="muted">Lets Hopback list VS Code's windows and bring one to the front.</p>
          <button onClick={() => api.openAccessibility()} disabled={st.accessibility}>
            Open System Settings
          </button>
        </Item>
        <Item done={st.plugin} title="Claude Code plugin">
          <p className="muted">Tells Hopback when a session works or waits for you. Run these in Claude Code, then start a new session:</p>
          {st.plugin_commands.map((c) => (
            <div className="row" key={c} style={{ marginBottom: 4 }}>
              <code className="grow">{c}</code>
              <button onClick={() => api.copyText(c)}>Copy</button>
            </div>
          ))}
        </Item>
        <Item done={banner} title="Notifications">
          <p className="muted">Banners appear when a session needs you.</p>
          <div className="row">
            <button
              onClick={() => {
                api.testBanner();
                setBanner(true);
              }}
            >
              Send test banner
            </button>
            <button onClick={() => api.openNotificationSettings()}>Notification settings</button>
          </div>
        </Item>
        <Item done={st.start_at_login} title="Start at login">
          <label className="row">
            <input type="checkbox" checked={st.start_at_login} onChange={(e) => api.setStartAtLogin(e.target.checked)} />
            <span>Open Hopback when you log in</span>
          </label>
        </Item>
      </div>
      <p className="muted">The history index may still be building in the background; search works once it is done.</p>
      <div>
        <button className="primary" onClick={() => api.finishWelcome()}>
          Finish
        </button>
      </div>
    </div>
  );
}
