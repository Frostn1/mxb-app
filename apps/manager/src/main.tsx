import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import { ErrorBoundary } from "@frost/shared/Components/ErrorBoundary";
import { I18nProvider } from "@/i18n";
import "./index.css";

/** The in-game overlay window loads the same bundle with `?overlay=1` (see
 *  `src-tauri/src/overlay.rs`) — same code, a much smaller surface. */
const isOverlay = new URLSearchParams(window.location.search).has("overlay");
const App = lazy(() => import("./App"));
const Overlay = lazy(() => import("./Components/Overlay/Overlay"));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {/* Outside the boundary's child so the boundary's own fallback copy is
        translated too. */}
    <I18nProvider>
      <ErrorBoundary label={isOverlay ? "overlay" : "root"}>
        <Suspense fallback={null}>{isOverlay ? <Overlay /> : <App />}</Suspense>
      </ErrorBoundary>
    </I18nProvider>
  </React.StrictMode>,
);
