import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { parseRoute } from "./lib/search";
import { Search } from "./views/Search";
import { SettingsView } from "./views/Settings";
import { Welcome } from "./views/Welcome";
import { Diagnostics } from "./views/Diagnostics";
import { About } from "./views/About";
import "./styles.css";

function App() {
  const [route, setRoute] = useState(() => parseRoute(window.location.hash));
  useEffect(() => {
    const on = () => setRoute(parseRoute(window.location.hash));
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  switch (route.view) {
    case "settings":
      return <SettingsView />;
    case "welcome":
      return <Welcome />;
    case "diagnostics":
      return <Diagnostics />;
    case "about":
      return <About />;
    default:
      return <Search initialMode={route.arg === "history" ? "history" : "open"} />;
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
