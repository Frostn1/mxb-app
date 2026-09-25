import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nContext, type I18nContextValue } from "@frost/shared/i18n/context";
import { Mxbmrp3Suggestion } from "../src/Components/Mxbmrp3/Mxbmrp3Suggestion";
import { createMxbmrp3Store, shouldSuggest, type Mxbmrp3Status } from "../src/lib/mxbmrp3";

const status = (over: Partial<Mxbmrp3Status>): Mxbmrp3Status => ({
  installed: false,
  dismissed: false,
  downloadUrl: "https://github.com/thomas4f/mxbmrp3/releases",
  ...over,
});

test("the suggestion shows only when the plugin is known to be missing and not waved off", () => {
  expect(shouldSuggest(status({}), false)).toBe(true);
  expect(shouldSuggest(status({ installed: true }), false)).toBe(false);
  // A game folder the app can't find says nothing about the plugin.
  expect(shouldSuggest(status({ installed: null }), false)).toBe(false);
  expect(shouldSuggest(null, false)).toBe(false);
});

test("don't ask again holds across launches, not now for the session", () => {
  expect(shouldSuggest(status({ dismissed: true }), false)).toBe(false);
  expect(shouldSuggest(status({}), true)).toBe(false);
});

test("every place the suggestion shows sees the same status and the same not now", async () => {
  let answer = status({});
  const store = createMxbmrp3Store(async () => answer);
  let heard = 0;
  const off = store.subscribe(() => heard++);

  await store.refresh();
  expect(shouldSuggest(store.get().status, store.get().snoozed)).toBe(true);

  // "Not now" on the setup card reaches the bar mounted after it.
  store.snooze();
  expect(shouldSuggest(store.get().status, store.get().snoozed)).toBe(false);

  // "Suggest it again" in Settings brings the bar back, not now included.
  store.unsnooze();
  await store.refresh();
  expect(shouldSuggest(store.get().status, store.get().snoozed)).toBe(true);

  // Installing it, then a re-check, takes it away everywhere.
  answer = status({ installed: true });
  await store.refresh();
  expect(shouldSuggest(store.get().status, store.get().snoozed)).toBe(false);
  expect(heard).toBe(5);

  off();
  store.snooze();
  expect(heard).toBe(5);
});

test("a check that fails suggests nothing", async () => {
  const store = createMxbmrp3Store(async () => {
    throw new Error("no config");
  });
  await store.refresh();
  expect(store.get().status).toBeNull();
});

test("the card explains itself and offers only a link, not now and never", () => {
  // The keys come back as themselves, so the markup shows which strings are used.
  const ctx: I18nContextValue = {
    locale: "en",
    resolved: "en",
    setLocale: () => {},
    t: (k: string) => k,
  };
  const html = renderToStaticMarkup(
    <I18nContext.Provider value={ctx}>
      <Mxbmrp3Suggestion
        downloadUrl="https://github.com/thomas4f/mxbmrp3/releases"
        onNotNow={() => {}}
        onNever={() => {}}
      />
    </I18nContext.Provider>,
  );
  for (const key of ["mxbmrp3.title", "mxbmrp3.what", "mxbmrp3.how", "mxbmrp3.get", "mxbmrp3.notNow", "mxbmrp3.never"]) {
    expect(html).toContain(key);
  }
  // A link out and two ways to close it: nothing that downloads or installs.
  expect(html.match(/<button/g)?.length).toBe(4);
  expect(html).not.toContain("download=");
});
