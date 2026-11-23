import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nContext, type I18nContextValue } from "@frost/shared/i18n/context";
import type { GraphicsPresetStatus } from "@frost/shared/types";
import { translate, type Locale } from "../src/i18n";
import { en } from "../src/i18n/locales/en";
import { de } from "../src/i18n/locales/de";
import { es } from "../src/i18n/locales/es";
import { fr } from "../src/i18n/locales/fr";
import { it as itIt } from "../src/i18n/locales/it";
import { ptBR } from "../src/i18n/locales/pt-BR";
import { sv } from "../src/i18n/locales/sv";
import { GraphicsPresetView } from "../src/Components/Settings/GraphicsPresetCard";

const LOCALES: Record<string, Record<string, string>> = {
  en,
  de,
  es,
  fr,
  it: itIt,
  "pt-BR": ptBR,
  sv,
};

const KEYS = Object.keys(en).filter((k) => k.startsWith("graphics."));

/** The placeholders a string fills in — a translation that drops one loses information. */
const vars = (s: string) => [...s.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]).sort();

describe("graphics preset strings", () => {
  test("there is a full set to check", () => {
    expect(KEYS.length).toBeGreaterThanOrEqual(25);
  });

  for (const [loc, dict] of Object.entries(LOCALES)) {
    test(`${loc} has every key, non-empty, with the same placeholders as English`, () => {
      for (const k of KEYS) {
        const s = dict[k];
        expect(typeof s, `${loc} ${k}`).toBe("string");
        expect(s.trim().length, `${loc} ${k}`).toBeGreaterThan(0);
        expect(vars(s), `${loc} ${k}`).toEqual(vars(en[k as keyof typeof en]));
      }
    });
  }
});

const base: GraphicsPresetStatus = {
  gameDir: "C:\\Games\\MX Bikes",
  gameFound: true,
  installed: false,
  choice: "off",
  disabled: false,
  existing: null,
  reshadeVersion: "6.8.0",
  photoFolder: "C:\\Games\\MX Bikes\\MXB Photos",
  hotkeys: { photo: "F9", dof: "F6", screenshot: "F11", overlay: "Home" },
  gpu: null,
  hiresFolder: "C:\\Users\\me\\Documents\\PiBoSo\\MX Bikes\\hrsshots",
  hiresPending: 0,
};

function render(status: GraphicsPresetStatus | null, opts: { busy?: "off" | "clean" | "photo"; running?: boolean; locale?: Locale } = {}) {
  const locale = opts.locale ?? "en";
  const ctx: I18nContextValue = {
    locale,
    resolved: locale,
    setLocale: () => {},
    t: (k: string, v?: Record<string, string | number>) =>
      translate(locale, k, { game: "MX Bikes", app: "MXB App", ...v }),
  };
  return renderToStaticMarkup(
    <I18nContext.Provider value={ctx}>
      <GraphicsPresetView
        status={status}
        busy={opts.busy ?? null}
        running={opts.running ?? false}
        onChoose={() => {}}
      />
    </I18nContext.Provider>,
  );
}

describe("GraphicsPresetView", () => {
  test("offers Off, MXB Clean and MXB Photo, with Off selected on a clean game", () => {
    const html = render(base);
    expect(html).toContain("MXB Clean");
    expect(html).toContain("MXB Photo");
    expect(html).toMatch(/aria-pressed="true"[^>]*>Off</);
    expect(html).toContain("reshade.me");
  });

  test("warns before replacing the player's own ReShade, naming its version", () => {
    const html = render({
      ...base,
      existing: { reshade: true, version: "6.7.3", otherDll: false, config: true },
    });
    expect(html).toContain("Your own ReShade 6.7.3");
    expect(html).toContain("Off puts it back");
  });

  test("an installed preset is the selected one", () => {
    const html = render({ ...base, installed: true, choice: "photo" });
    expect(html).toMatch(/aria-pressed="true"[^>]*>MXB Photo</);
    expect(html).toContain("screenshots");
  });

  test("says it is downloading while the first install runs", () => {
    expect(render(base, { busy: "clean" })).toContain("Downloading ReShade 6.8.0 from reshade.me");
  });

  test("asks for the game folder when there is none", () => {
    expect(render({ ...base, gameFound: false })).toContain("Set your MX Bikes folder first");
    expect(render(null)).toContain("Set your MX Bikes folder first");
  });

  test("photo mode lists the keys as bound, with the open-folder button", () => {
    const html = render({ ...base, installed: true, choice: "photo", gpu: "nvidia" });
    expect(html).toContain("Photo mode");
    for (const key of ["F9", "F6", "F11", "F10", "Home"]) expect(html).toContain(`>${key}</kbd>`);
    expect(html).toContain("4K/8K");
    expect(html).toContain("Open photos folder");
    expect(html).toContain("MXB Photos");
    expect(html).toContain("NVIDIA Control Panel");
  });

  test("photo mode shows the key the player rebound and skips an unbound one", () => {
    const html = render({
      ...base,
      installed: true,
      choice: "clean",
      hotkeys: { photo: "Ctrl+Shift+P", dof: "", screenshot: "F11", overlay: "Home" },
    });
    expect(html).toContain(">Ctrl+Shift+P</kbd>");
    expect(html).not.toContain(">F6</kbd>");
    expect(html).not.toContain("Depth of field on or off");
  });

  test("the super-resolution tip follows the GPU", () => {
    const on = { ...base, installed: true, choice: "clean" as const };
    expect(render({ ...on, gpu: "amd" })).toContain("Virtual Super Resolution");
    const other = render({ ...on, gpu: "intel" });
    expect(other).toContain("NVIDIA DSR or AMD VSR");
    expect(other).not.toContain("NVIDIA Control Panel");
  });

  test("F10 shots: says what they are, and offers PNG conversion only when some wait", () => {
    const on = { ...base, installed: true, choice: "photo" as const };
    const none = render(on);
    expect(none).toContain("no HUD and no ReShade effects");
    expect(none).toContain("Open F10 shots");
    expect(none).not.toContain("F10 shots to PNG");
    expect(render({ ...on, hiresPending: 2 })).toContain("F10 shots to PNG (2)");
  });

  test("no photo mode block while the preset is off or not installed", () => {
    expect(render(base)).not.toContain("Photo mode");
    expect(render({ ...base, installed: true, choice: "off" })).not.toContain("Photo mode");
  });

  test("renders in every language", () => {
    for (const loc of Object.keys(LOCALES) as Locale[]) {
      const html = render({ ...base, existing: { reshade: false, version: null, otherDll: true, config: false } }, { locale: loc });
      expect(html).toContain("MXB Clean");
      expect(html).toContain("opengl32.dll");
      const on = render({ ...base, installed: true, choice: "photo" }, { locale: loc });
      expect(on).toContain(">F9</kbd>");
      expect(on).toContain(translate(loc, "graphics.openPhotos"));
    }
  });
});
