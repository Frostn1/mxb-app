import js from "@eslint/js";
import globals from "globals";
import react from "eslint-plugin-react";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  // Vite rewrites its config to a timestamped sibling on reload; it's gitignored
  // build machinery, not source.
  {
    ignores: [
      "dist",
      "src-tauri/target",
      "vite.config.ts.timestamp-*.mjs",
      // Separate deployable with its own tsconfig, lint rules and generated types — the
      // desktop app's browser-oriented config only produces noise against Workers code.
      "control-plane",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    // Standalone Node maintenance scripts run outside the browser bundle.
    files: ["scripts/**/*.{js,mjs}"],
    languageOptions: { globals: globals.node },
  },
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: { react, "react-hooks": reactHooks },
    rules: {
      ...react.configs.flat.recommended.rules,
      // The two classic hook rules only. v7's `recommended` also enables the React
      // Compiler ruleset (set-state-in-effect, immutability, …), which flags ~50
      // pre-existing patterns across the app — worth its own pass, not a lint-config fix.
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      "react/react-in-jsx-scope": "off",
      "react/prop-types": "off",
    },
    settings: { react: { version: "detect" } },
  },
  {
    // react-three-fiber renders three.js objects as JSX (`<mesh castShadow>`,
    // `<ambientLight intensity>`), which eslint-plugin-react judges against the DOM and
    // flags wholesale. @react-three/fiber ships types for them, so tsc is the real check.
    files: [
      "src/Components/Viewer/ModelViewer.tsx",
      "src/Components/Viewer/TrackViewer.tsx",
    ],
    rules: { "react/no-unknown-property": "off" },
  },
);
