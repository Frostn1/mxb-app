import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import CoachOverlay from "./Components/Overlay/CoachOverlay";
import "./index.css";

/** The in-game overlay window loads this same bundle with `?overlay=1` (`overlay.rs`). */
const isOverlay = new URLSearchParams(window.location.search).has("overlay");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isOverlay ? <CoachOverlay /> : <App />}</React.StrictMode>,
);
