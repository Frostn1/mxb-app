import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./Components/ErrorBoundary";
import { I18nProvider } from "./i18n";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {/* Outside the boundary's child so the boundary's own fallback copy is
        translated too. */}
    <I18nProvider>
      <ErrorBoundary label="root">
        <App />
      </ErrorBoundary>
    </I18nProvider>
  </React.StrictMode>,
);
