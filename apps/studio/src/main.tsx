import React from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { providePluginRecorder } from "@frost/shared/lib/pluginHost";
import {
  replayRecord,
  replayStatus,
  replayStop,
  REPLAY_EVENT,
  type ReplayStatus,
} from "@frost/shared/api/replay";
import App from "./App";
import "./index.css";

// The recorder a paid plugin's panels are handed — the Replay Mod's **Record** button, if it
// draws one. Provided here rather than imported inside the plugin host, because this is the
// only binary that registers those commands: the host is shared with the mod manager, where
// they would be calls that could never work.
providePluginRecorder({
  status: replayStatus,
  start: replayRecord,
  stop: replayStop,
  onChange: (fn) => {
    // The unlisten arrives a tick late; the returned function waits for it rather than
    // dropping it, so a panel that unmounts immediately still stops listening.
    const pending = listen<ReplayStatus>(REPLAY_EVENT, (e) => fn(e.payload));
    return () => void pending.then((off) => off()).catch(() => {});
  },
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
