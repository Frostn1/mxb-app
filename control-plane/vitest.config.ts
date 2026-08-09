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
  test: {
    // The Worker's tests are plain unit tests over pure functions; nothing here needs the
    // workers pool.
    include: ["test/**/*.test.ts"],
  },
};
