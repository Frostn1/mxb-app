import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Overlay from "./Components/Overlay/Overlay";
import { ErrorBoundary } from "./Components/ErrorBoundary";
import "./index.css";

/** The in-game overlay window loads the same bundle with `?overlay=1` (see
 *  `src-tauri/src/overlay.rs`) — same code, a much smaller surface. */
const isOverlay = new URLSearchParams(window.location.search).has("overlay");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary label={isOverlay ? "overlay" : "root"}>
      {isOverlay ? <Overlay /> : <App />}
    </ErrorBoundary>
  </React.StrictMode>,
);
