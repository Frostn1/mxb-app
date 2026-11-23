import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  backgroundDecls,
  buttonDecls,
  compileAppearance,
  cssColor,
  cssLength,
  fontFamilyName,
  colorwayActive,
  tokenDecls,
} from "@frost/shared/extensions/appearance";
import { resolveInPackage, themeCss, viewCsp } from "@frost/shared/extensions/sandbox";
import { PERMISSIONS, METHOD_PERMISSION } from "@frost/shared/extensions/permissions";
import { toExtFriend, toExtServer } from "@frost/shared/extensions/data";
import { cleanLabel } from "@frost/shared/extensions/manifest";
import { tierLabelText } from "@frost/shared/extensions/trust";

test("a colorway is inactive while an extension theme is picked", () => {
  expect(colorwayActive("frost", "frost", null)).toBe(true);
  expect(colorwayActive("frost", "frost", "maker.theme/dark")).toBe(false);
  expect(colorwayActive("slate", "frost", null)).toBe(false);
});

test("catalog trust tiers have one clear label", () => {
  expect(tierLabelText(true, false)).toBe("Verified");
  expect(tierLabelText(true, true)).toBe("Verified · Suggested");
  expect(tierLabelText(false, true)).toBe("Suggested");
  expect(tierLabelText(false, false)).toBe("Unverified");
});

test("colours: plain values pass, anything that can load or reference something fails", () => {
  expect(cssColor("#0A84FF")).toBe("#0a84ff");
  expect(cssColor("rgba(10, 132, 255, 0.16)")).toBe("rgba(10, 132, 255, 0.16)");
  expect(cssColor("oklch(0.7 0.1 240deg / 50%)")).not.toBeNull();
  for (const bad of [
    "url(https://evil.example/x.png)",
    "red; background: url(x)",
    "var(--primary)",
    "rgb(1,2,3) url(x)",
    "expression(alert(1))",
    "rgb(calc(1),2,3)",
    "image-set(x)",
    "#12",
    "red",
    "rgb(1 2 3) } body { display:none",
    "rgb(attr x)",
  ]) {
    expect(cssColor(bad)).toBeNull();
  }
});

test("only known tokens become declarations", () => {
  const decls = tokenDecls({ primary: "#ff0000", "--x": "#fff", "background; color": "#000", constructor: "#111" } as never);
  expect(decls).toEqual(["--primary: #ff0000;"]);
});

test("lengths are bounded", () => {
  expect(cssLength("12px", 32)).toBe("12px");
  expect(cssLength("0.5rem", 32)).toBe("0.5rem");
  expect(cssLength("100px", 32)).toBeNull();
  expect(cssLength("1px; color: red", 32)).toBeNull();
  expect(cssLength("calc(1px)", 32)).toBeNull();
});

test("buttons and backgrounds are built, not copied", () => {
  expect(buttonDecls({ id: "p", label: "P", radius: "999px", uppercase: true, letterSpacing: 0.06, weight: 700 })).toEqual([
    "border-radius: 999px !important;",
    "text-transform: uppercase !important;",
    "letter-spacing: 0.06em !important;",
    "font-weight: 700 !important;",
  ]);
  expect(buttonDecls({ id: "p", label: "P", letterSpacing: 5 as never })).toEqual([]);
  // A data URL we didn't build (wrong mime, injected quote) is dropped.
  expect(backgroundDecls({ id: "b", label: "B" }, 'data:text/html;base64,PHNjcmlwdD4=')).toEqual([]);
  expect(backgroundDecls({ id: "b", label: "B" }, 'data:image/png;base64,AAA=") ; x:url("https://e')).toEqual([]);
  const ok = backgroundDecls({ id: "b", label: "B", opacity: 0.2 }, "data:image/png;base64,AAAA");
  expect(ok[0]).toBe('background-image: url("data:image/png;base64,AAAA");');
  expect(ok).toContain("opacity: 0.2;");
  const g = backgroundDecls({ id: "b", label: "B", gradient: { angle: 90, stops: ["#000", "url(x)", "#fff"] } }, null);
  expect(g[0]).toBe("background-image: linear-gradient(90deg, #000, #fff);");
});

test("font families are ours and can't break out of the string", () => {
  expect(fontFamilyName("someone.theme", "Display")).toBe("mxbext-someone-theme-display");
  expect(fontFamilyName('a.b"; x', "y")).toMatch(/^mxbext-[a-z0-9-]+$/);
  const { css } = compileAppearance({ fonts: { body: 'Arial"; } body { x' } });
  expect(css).toBe("");
});

test("the compiled sheet is keyed on attributes and contains only generated rules", () => {
  const { css, attrs } = compileAppearance({
    theme: { ext: "a.b", theme: { id: "t", label: "T", dark: { primary: "#ff4a3d", background: "url(x)" }, radius: "4px" } },
    buttons: { id: "b", label: "B", radius: "0px" },
  });
  expect(attrs).toEqual({ "data-ext-theme": "", "data-ext-buttons": "" });
  expect(css).toContain("html.dark[data-ext-theme] { --primary: #ff4a3d; }");
  expect(css).toContain("--radius: 4px;");
  expect(css).not.toContain("url(");
});

test("views can only reach files inside their package", () => {
  expect(resolveInPackage("view/index.html", "app.js")).toBe("view/app.js");
  expect(resolveInPackage("view/index.html", "../shared/a.css")).toBe("shared/a.css");
  expect(resolveInPackage("view/index.html", "../../etc/passwd")).toBeNull();
  expect(resolveInPackage("view/index.html", "https://evil.example/x.js")).toBeNull();
  expect(resolveInPackage("view/index.html", "//evil.example/x.js")).toBeNull();
  expect(resolveInPackage("view/index.html", "/abs.js")).toBeNull();
  expect(resolveInPackage("view/index.html", "javascript:alert(1)")).toBeNull();
  expect(resolveInPackage("index.html", "data:text/javascript,1")).toBeNull();
});

test("a view's policy allows no network unless hosts were granted", () => {
  const none = viewCsp([]);
  expect(none).toContain("connect-src 'none'");
  expect(none).toContain("default-src 'none'");
  expect(none).toContain("frame-src 'none'");
  expect(none).toContain("base-uri 'none'");
  const some = viewCsp(["https://api.example.com", "http://bad.example", "https://x.com/path", "*"]);
  expect(some).toContain("connect-src https://api.example.com;");
  expect(some).not.toContain("bad.example");
  expect(some).not.toContain("x.com/path");
});

test("theme tokens handed to a view are re-checked", () => {
  const css = themeCss({ "--primary": "#0a84ff", "--background": "url(https://e/x)", "--evil": "#fff", "font-family": 'Barlow, "Segoe UI", sans-serif' }, "dark");
  expect(css).toContain("--primary: #0a84ff;");
  expect(css).toContain('body { font-family: Barlow, "Segoe UI", sans-serif; }');
  expect(css).not.toContain("url(");
  expect(css).not.toContain("--evil");
  expect(themeCss({ "font-family": "serif; } body { display:none" }, "dark")).not.toContain("display:none");
  expect(themeCss({ "font-family": "var(--font-family)" }, "dark")).not.toContain("font-family");
});

test("the permission list matches the backend's", () => {
  const rust = readFileSync(join(import.meta.dir, "../../../crates/core/src/extensions.rs"), "utf8");
  const block = rust.slice(rust.indexOf("pub const PERMISSIONS"), rust.indexOf("];", rust.indexOf("pub const PERMISSIONS")));
  const names = [...block.matchAll(/"([a-zA-Z.]+)"/g)].map((m) => m[1]);
  expect(names).toEqual([...PERMISSIONS]);
  for (const p of Object.values(METHOD_PERMISSION)) expect(PERMISSIONS).toContain(p);
});

test("data handed to extensions carries no account ids, addresses or tokens", () => {
  const f = toExtFriend(
    {
      riderName: "Kid",
      presence: { serverName: "EU #1", track: "Club", riders: 12, address: "1.2.3.4:5" } as never,
      lastSeenAt: 1,
      accountId: "acct_123",
    } as never,
    10,
  );
  expect(f).toEqual({ name: "Kid", online: true, server: "EU #1", track: "Club", riders: 12, lastSeenAt: 1 });
  const s = toExtServer({ name: "S", address: "1.2.3.4:54210", lanAddress: "192.168.1.5:1", players: 3, maxPlayers: 40, pingMs: 20, passworded: false, location: "EU", track: "T", session: "Race", rating: "", token: "secret" } as never);
  expect(JSON.stringify(s)).not.toContain("192.168");
  expect(JSON.stringify(s)).not.toContain("secret");
});

test("labels lose control and bidi characters", () => {
  expect(cleanLabel("Good‮evil\u0000")).toBe("Goodevil");
  expect(cleanLabel("x".repeat(100))).toHaveLength(40);
});

test("play button and component tokens compile to bounded CSS only", async () => {
  const { playDecls, componentRules, compileAppearance } = await import("@frost/shared/extensions/appearance");
  expect(playDecls({ radius: "3px", background: "#facc15", foreground: "red", uppercase: true, width: "wide" })).toEqual([
    "border-radius: 3px !important;",
    "background-color: #facc15 !important;",
    "text-transform: uppercase;",
    "padding-left: 2.25rem !important; padding-right: 2.25rem !important;",
  ]);
  expect(playDecls({ radius: "1px; } body { display:none", background: "url(x)" })).toEqual([]);
  const rules = componentRules({ cardRadius: "3px", inputRadius: "99px", borderWidth: 9, tabMarker: "block" }, "html");
  expect(rules[0]).toBe("html[data-ext-components] :is(.bg-card, .mxb-card):not(.mxb-server-sort-row) { border-radius: 3px !important; }");
  expect(rules.join("\n")).not.toContain("99px");
  expect(rules.join("\n")).toContain(".mxb-tab-marker");
  expect(rules.join("\n")).not.toContain(".mxb-server-sort-row button");
  const { attrs, css } = compileAppearance({ play: { label: "x" } as never, components: {} });
  expect(attrs).toEqual({});
  expect(css).toBe("");
});
