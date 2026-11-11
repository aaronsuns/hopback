import { useEffect, useState } from "react";
import { api } from "../api";

export function About() {
  const [version, setVersion] = useState("");
  useEffect(() => {
    api.version().then(setVersion);
  }, []);
  return (
    <div className="page" style={{ alignItems: "center", textAlign: "center" }}>
      <h1>Hopback</h1>
      <div className="muted">Version {version}</div>
      <p>Get back to your Claude Code sessions in VS Code.</p>
      <p className="muted">
        Independent open-source project, MIT licence. Not affiliated with or endorsed by Anthropic.
      </p>
      <div className="row">
        <button onClick={() => api.openUrl("https://hopback.zexa.se")}>Website</button>
        <button onClick={() => api.openUrl("https://github.com/aaronsuns/hopback")}>Source</button>
      </div>
    </div>
  );
}
