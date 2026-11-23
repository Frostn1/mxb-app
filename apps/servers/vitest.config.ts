import { defineConfig } from "vitest/config";
import path from "node:path";

export default defineConfig({
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    // Three CI runners share one PC with a cargo build; a worker per core there starves tests.
    ...(process.env.CI ? { maxWorkers: 4, minWorkers: 1 } : {}),
  },
});
