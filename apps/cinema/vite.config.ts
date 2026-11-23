import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// @tauri-apps/cli sets TAURI_DEV_HOST when developing against a physical device.
const host = process.env.TAURI_DEV_HOST;

// 1470: the manager has 1420, the studio 1430, Coach 1440, the converter 1460.
const port = Number(process.env.FROST_CINEMA_DEV_PORT) || 1470;

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@frost/shared": path.resolve(__dirname, "../../packages/shared/src"),
      // Coach's replay clock, shared as it stands rather than copied.
      "@coach": path.resolve(__dirname, "../coach/src"),
    },
  },

  clearScreen: false,
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: port + 1 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
}));
