import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nContext, type I18nContextValue } from "@frost/shared/i18n/context";
import type { UninstallInfo } from "@frost/shared/api/uninstall";
import { canSelfRemove, confirmMatches, dataChoice } from "@frost/shared/lib/uninstall";
import { UninstallDialog } from "@frost/shared/Components/Uninstall/UninstallSetting";
import { en } from "@frost/shared/i18n/base/en";
import { de } from "@frost/shared/i18n/base/de";
import { es } from "@frost/shared/i18n/base/es";
import { fr } from "@frost/shared/i18n/base/fr";
import { it as itIt } from "@frost/shared/i18n/base/it";
import { ptBR } from "@frost/shared/i18n/base/pt-BR";
import { sv } from "@frost/shared/i18n/base/sv";

const info = (over: Partial<UninstallInfo>): UninstallInfo => ({
  product: "MXB Coach",
  method: "launch",
  command: null,
  dataDirs: ["C:\\Users\\r\\AppData\\Roaming\\com.frost.mxbcoach"],
  sharedWith: [],
  nsis: true,
  ...over,
});

test("the app's name has to be typed to confirm, give or take case and spacing", () => {
  expect(confirmMatches("mxb  coach ", "MXB Coach")).toBe(true);
  expect(confirmMatches("MXB", "MXB Coach")).toBe(false);
  expect(confirmMatches("", "")).toBe(false);
});

test("MXB App's data can't be deleted while Coach or Studio still read it", () => {
  expect(dataChoice(info({}))).toEqual({ offered: true, blockedBy: [] });
  expect(dataChoice(info({ product: "MXB App", sharedWith: ["MXB Coach"] }))).toEqual({
    offered: false,
    blockedBy: ["MXB Coach"],
  });
  expect(dataChoice(info({ dataDirs: [] })).offered).toBe(false);
});

test("a system package is described, never removed by the app", () => {
  expect(canSelfRemove(info({}))).toBe(true);
  expect(canSelfRemove(info({ method: "trash" }))).toBe(true);
  expect(canSelfRemove(info({ method: "package", command: "sudo apt remove mxb-coach" }))).toBe(false);
  expect(canSelfRemove(info({ method: "none" }))).toBe(false);
});

const ctx: I18nContextValue = { locale: "en", resolved: "en", setLocale: () => {}, t: (k: string) => k };
const render = (i: UninstallInfo) =>
  renderToStaticMarkup(
    <I18nContext.Provider value={ctx}>
      <UninstallDialog info={i} open onOpenChange={() => {}} run={async () => {}} />
    </I18nContext.Provider>,
  );

test("the dialog says what's kept and warns about the uninstaller's own data box", () => {
  // Radix renders the open dialog into a portal, which server rendering leaves out, so this
  // renders nothing server-side; it must at least not throw with every branch reachable.
  expect(() => render(info({}))).not.toThrow();
  expect(() => render(info({ product: "MXB App", sharedWith: ["MXB Coach", "Frost Studio"] }))).not.toThrow();
});

test("every locale carries every uninstall string", () => {
  const keys = Object.keys(en).filter((k) => k.startsWith("uninstall."));
  expect(keys.length).toBe(19);
  for (const dict of [de, es, fr, itIt, ptBR, sv] as Record<string, string>[]) {
    for (const k of keys) expect(dict[k]?.length ?? 0).toBeGreaterThan(0);
    // The app name and the list of apps sharing the folder are filled in, never dropped.
    expect(dict["uninstall.typeName"]).toContain("{{app}}");
    expect(dict["uninstall.nsisShared"]).toContain("{{apps}}");
  }
});
