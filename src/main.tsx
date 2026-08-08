import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Overlay from "./Components/Overlay/Overlay";
import { ErrorBoundary } from "./Components/ErrorBoundary";
import { I18nProvider } from "./i18n";
import "./index.css";

/** The in-game overlay window loads the same bundle with `?overlay=1` (see
 *  `src-tauri/src/overlay.rs`) — same code, a much smaller surface. */
const isOverlay = new URLSearchParams(window.location.search).has("overlay");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {/* Outside the boundary's child so the boundary's own fallback copy is
        translated too. */}
    <I18nProvider>
      <ErrorBoundary label={isOverlay ? "overlay" : "root"}>
        {isOverlay ? <Overlay /> : <App />}
      </ErrorBoundary>
    </I18nProvider>
  </React.StrictMode>,
);
