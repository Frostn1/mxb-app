/**
 * Stops vitest walking up the tree.
 *
 * Without a config here it finds the repo root's `vite.config.ts` — the desktop app's — and
 * tries to load it against this package's `node_modules`, where the app's plugins are not
 * installed. That only shows up somewhere the root deps are absent, which is to say in CI
 * and never on a dev machine.
 *
 * A plain object rather than `defineConfig`, so this file itself imports nothing.
 */
export default {
  // Wrangler imports `.md` as text (the `rules` in wrangler.jsonc); this does the same here.
  plugins: [
    {
      name: "text",
      transform(code: string, id: string) {
        return id.endsWith(".md") ? `export default ${JSON.stringify(code)};` : undefined;
      },
    },
  ],
  test: {
    // The Worker's tests are plain unit tests over pure functions; nothing here needs the
    // workers pool.
    include: ["test/**/*.test.ts"],
  },
};
