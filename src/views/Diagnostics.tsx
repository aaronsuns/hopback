import { useEffect, useState } from "react";
import { api, Check } from "../api";

export function Diagnostics() {
  const [data, setData] = useState<{ checks: Check[]; report: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const load = () => api.diagnostics().then(setData);
  useEffect(() => {
    load();
  }, []);
  if (!data) return <div className="page muted">Checking…</div>;
  return (
    <div className="page">
      <h1>Diagnostics</h1>
      <div className="card">
        {data.checks.map((c) => (
          <div className="row" key={c.name} style={{ alignItems: "flex-start" }}>
            <span className={`check ${c.ok === true ? "ok" : c.ok === false ? "bad" : "muted"}`}>
              {c.ok === true ? "✓" : c.ok === false ? "✗" : "•"}
            </span>
            <div className="grow">
              <strong>{c.name}</strong> <span className="muted">{c.detail}</span>
              {c.ok === false && c.hint && <div className="muted">{c.hint}</div>}
            </div>
          </div>
        ))}
      </div>
      <div className="row">
        <button
          className="primary"
          onClick={async () => {
            await api.copyText(data.report);
            setCopied(true);
          }}
        >
          {copied ? "Copied" : "Copy report"}
        </button>
        <button onClick={load}>Check again</button>
        <span className="muted">The report never contains conversation text.</span>
      </div>
    </div>
  );
}
