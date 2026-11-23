import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { setDefaultNavStyle, setLeftOrbitsInBlender } from "@frost/shared/lib/viewportNav";
import "./index.css";

// The preview's camera becomes the Blender shot, so it moves like Blender's viewport; a left
// drag orbits too, since nothing in the view is there to be selected.
setDefaultNavStyle("blender");
setLeftOrbitsInBlender(true);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
