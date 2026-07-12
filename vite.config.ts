import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @tauri-apps/cli sets TAURI_DEV_HOST when developing against a physical device.
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  css: {
    preprocessorOptions: {
      scss: { api: "modern-compiler" },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`.
  //
  // 1. prevent Vite from obscuring Rust errors
  clearScreen: false,
  // 2. Tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
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
