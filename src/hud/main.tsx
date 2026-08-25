import React from "react";
import ReactDOM from "react-dom/client";
import HudView from "./HudView";
import "../styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <HudView />
  </React.StrictMode>,
);
