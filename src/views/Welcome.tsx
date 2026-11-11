import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, InstallResult, WelcomeStatus } from "../api";
import { notificationsDone } from "../lib/welcome";

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
  const [notifAllowed, setNotifAllowed] = useState<boolean | null>(null);
  const [installing, setInstalling] = useState(false);
  const [install, setInstall] = useState<InstallResult | null>(null);

  const checkNotifications = useCallback(async () => {
    try {
      setNotifAllowed(await api.notificationPermission());
    } catch {
      setNotifAllowed(null);
    }
  }, []);

  // Re-check when the window gains focus: the user may have just changed it in
  // System Settings, and after a restart the permission must show as granted.
  useEffect(() => {
    checkNotifications();
    const onFocus = () => checkNotifications();
    window.addEventListener("focus", onFocus);
    const un = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) checkNotifications();
    });
    return () => {
      window.removeEventListener("focus", onFocus);
      un.then((f) => f()).catch(() => {});
    };
  }, [checkNotifications]);

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

  async function runInstall() {
    setInstalling(true);
    setInstall(null);
    try {
      setInstall(await api.installPlugin());
      setSt(await api.welcomeStatus());
    } catch (e) {
      setInstall({ ok: false, claude_found: true, output: String(e) });
    } finally {
      setInstalling(false);
    }
  }

  return (
    <div className="page">
      <h1>Welcome to Hopback</h1>
      <p className="muted">A few things make Hopback work. This list ticks itself off as you go.</p>
      <div className="card">
        <Item done={st.accessibility} title="Accessibility">
          <p className="muted">
            Lets Hopback list VS Code's windows and bring one to the front. Click Open System Settings, find Hopback
            in the list and switch it on.
          </p>
          <p className="muted small">
            If Hopback is already switched on but this step stays unticked, select Hopback, remove it with −, then
            add it again with + (/Applications/Hopback.app). That happens after reinstalling an older copy.
          </p>
          <button onClick={() => api.openAccessibility()} disabled={st.accessibility}>
            Open System Settings
          </button>
        </Item>
        <Item done={st.plugin === "ready"} title="Claude Code plugin">
          <p className="muted">Tells Hopback when a session is working or waiting for you.</p>
          {st.plugin === "ready" ? (
            <p className="ok">Installed and working.</p>
          ) : st.plugin === "installed" || install?.ok ? (
            <p>
              Installed. Now open a new Claude Code session (or reload the VS Code window) — sessions that were already
              open don't load new plugins.
            </p>
          ) : (
            <>
              <div className="row">
                <button className="primary" disabled={installing} onClick={runInstall}>
                  Install plugin
                </button>
                {installing && (
                  <span className="muted">
                    <span className="spinner" /> Installing…
                  </span>
                )}
              </div>
              {install && !install.ok && !install.claude_found && (
                <p className="error">
                  Hopback could not find <code>claude</code>. Set its path in Settings → General, or use the manual way
                  below.
                </p>
              )}
              {install && !install.ok && install.claude_found && (
                <div className="error">
                  Install failed:
                  <pre className="output mono">{install.output || "no output"}</pre>
                </div>
              )}
            </>
          )}
          {st.plugin !== "ready" && (
            <div className="small" style={{ marginTop: 8 }}>
              <p className="muted" style={{ margin: "0 0 4px" }}>
                Or install it by hand in Terminal:
              </p>
              {st.plugin_commands.map((c) => (
                <div className="row" key={c} style={{ marginBottom: 4 }}>
                  <code className="grow">{c}</code>
                  <button className="small" onClick={() => api.copyText(c)}>
                    Copy
                  </button>
                </div>
              ))}
              <p className="muted" style={{ margin: "4px 0 0" }}>
                The Claude Code panel in VS Code has no /plugin command; run these in Terminal.
              </p>
            </div>
          )}
        </Item>
        <Item done={notificationsDone(notifAllowed, banner)} title="Notifications">
          <p className="muted">
            Banners appear when a session needs you. Click Send test banner. If no banner appears, click Notification
            settings and allow Hopback.
          </p>
          <div className="row">
            <button
              onClick={() => {
                api.testBanner();
                setBanner(true);
                setTimeout(checkNotifications, 1500);
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
