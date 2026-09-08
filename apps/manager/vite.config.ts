import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// @tauri-apps/cli sets TAURI_DEV_HOST when developing against a physical device.
const host = process.env.TAURI_DEV_HOST;

// Several worktrees of this repo get run at once, and `strictPort` means the second one to
// start just dies. `MXB_DEV_PORT` moves this instance out of the way; Tauri has to be told
// the same number, so it travels with a matching `--config '{"build":{"devUrl":…}}'`.
const port = Number(process.env.MXB_DEV_PORT) || 1420;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`.
  //
  // 1. prevent Vite from obscuring Rust errors
  clearScreen: false,
  // 2. Tauri expects a fixed port, fail if that port is not available
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: port + 1,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  // 4. to make use of `TAURI_ENV_*` and app env variables in the frontend
  envPrefix: ["VITE_", "TAURI_ENV_"],
}));
