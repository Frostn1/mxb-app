import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Button } from "../ui/button";

/**
 * The Steam sign-in page — shared by every app in the lineup.
 *
 * When the deployment requires a Valve-confirmed Steam sign-in (`MXB_REQUIRE_STEAM` on the
 * control plane), the startup gate answers `signin` and the Rust side emits
 * `mxb-signin-required`. This overlay covers the whole window on that event and nothing behind
 * it can be used until the account is linked — the enforcement is the gate, this is the welcome
 * page that explains why sign-in is needed.
 *
 * The flow reuses the commands the secure-content link already had: `steam_link_start` returns a
 * Steam OpenID URL, the browser half lands on the control plane and sets the Steam id, and
 * `steam_link_status` reports when it has. Once linked we call `recheck_gate`, which re-asks the
 * gate; a clean verdict emits `mxb-signin-required { required: false }` and the wall comes down.
 *
 * Honest throughout: this is a requirement to meet, not the disguised block a banned install
 * gets (that one closes the app instead of ever reaching here).
 *
 * The one rule this page has to keep: **the button is never dead**. It is the only control on
 * the only thing on screen, so a state it cannot be clicked out of is the app not opening. It
 * used to be disabled for the whole wait — minutes, on a sign-in that had already failed in the
 * browser, with no way to start another and no way to tell that from the app having hung. Now it
 * only goes quiet for the moment it takes to open the browser, and every wait can be restarted
 * over the top of itself.
 */
export default function SigninGate() {
  const [required, setRequired] = useState(false);
  /** The browser is being opened. The one moment when a second click has nothing to do. */
  const [opening, setOpening] = useState(false);
  /** The browser is open and we are waiting on Steam. Clicking again starts a fresh sign-in. */
  const [waiting, setWaiting] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  /**
   * The sign-in URL we last minted, kept so it can be shown.
   *
   * `openUrl` resolving is not proof a browser opened. On Windows it hands the URL to the
   * shell and reports success as soon as the shell accepts it, which it does when there is no
   * default browser association and when an elevated app cannot reach the desktop session. The
   * rows show what that looks like: one account minted nineteen sign-in URLs over two days and
   * not one of them ever reached `/v1/steam/start` — nineteen clicks, no browser, and an app
   * that said "Waiting for Steam to confirm it's you…" every time. So the URL is shown rather
   * than only opened, and the sign-in stops depending on a call that cannot report this failure.
   */
  const [url, setUrl] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  /**
   * Which attempt is the live one. Bumped by every click, so a poll left over from an earlier
   * attempt sees it has been superseded and stops touching the screen — the alternative being an
   * old loop's "Steam hasn't come back" landing on top of the attempt the person is watching.
   */
  const attempt = useRef(0);

  useEffect(() => {
    const pending = listen<{ required: boolean; message: string }>("mxb-signin-required", (e) => {
      setRequired(e.payload.required);
      if (!e.payload.required) {
        attempt.current++;
        setOpening(false);
        setWaiting(false);
        setNote(null);
        setUrl(null);
      }
    });

    // Ask for the verdict rather than only waiting to be told it.
    //
    // `gate::check` is spawned from Rust `setup`, which runs before this webview exists, and a
    // Tauri event is delivered to the listeners attached at that instant — there is no buffer
    // and no replay. The gate's round trip is a couple of hundred milliseconds; mounting a
    // React app of this size on a cold start is often slower. So the `signin` verdict was
    // routinely emitted into an empty room, and the wall then never appeared at all: the update
    // says you must sign in with Steam, and the app gives you nothing to sign in with. That is
    // the shape of "can't sign in after updating", and it is a race, which is why it looked
    // intermittent.
    //
    // `gate_verdict` re-sends the verdict the gate already reached, so asking once from here —
    // after `listen` has resolved, because the listener is not attached until it does — closes
    // the half of the handshake that could be missed. It is a replay, not a second check: it
    // asks the service for nothing, and when no verdict has been reached yet it does nothing,
    // because the check still in flight will emit to the listener that now exists.
    void pending.then(() => invoke("gate_verdict")).catch(() => {});

    return () => {
      void pending.then((off) => off()).catch(() => {});
    };
  }, []);

  const signIn = async () => {
    if (opening) return;
    // Synchronous, before any await: two clicks landing in one tick both read the ref, and the
    // second supersedes the first rather than running beside it.
    const mine = ++attempt.current;
    const mineStill = () => attempt.current === mine;
    setOpening(true);
    setWaiting(false);
    setNote("Opening Steam in your browser…");

    let minted: string;
    try {
      minted = await invoke<string>("steam_link_start");
    } catch (e) {
      if (!mineStill()) return;
      setOpening(false);
      setNote(typeof e === "string" ? e : "Couldn't start the sign-in. Try again.");
      return;
    }
    if (!mineStill()) return;
    // Shown from here on, whatever the browser does. A minted URL is good for an hour before
    // it is even opened (`LOGIN_START_TTL_MS`), so there is no hurry and nothing is wasted by
    // putting it on screen.
    setUrl(minted);
    setCopied(false);

    // A failure here is worth saying out loud, but it is not the end of the sign-in: the link
    // below still works, and the poll below still watches for it. This used to return, leaving
    // the rider with an error and no way through.
    try {
      await openUrl(minted);
    } catch {
      if (!mineStill()) return;
      setNote("Couldn't open your browser. Use the link below to sign in.");
    }
    if (!mineStill()) return;

    // The browser is open, so the button comes back: from here the useful thing a second click
    // does is open Steam again, which is exactly what somebody looking at a browser tab that
    // said "sign-in expired" needs.
    setOpening(false);
    setWaiting(true);
    setNote((n) => n ?? "Waiting for Steam to confirm it's you…");

    // Poll until the link lands, then let the gate have the final word. The ceiling matches the
    // control plane's own sign-in window, which is now thirty minutes counted from the browser
    // arriving at Steam (`signin.ts`): stopping earlier than the service does leaves a sign-in
    // that is still perfectly valid — a Steam Guard prompt, a password typed slowly — with
    // nothing watching for it, which is the app going quiet on a sign-in that then works.
    //
    // Every two seconds for the first minute, because that is when almost every sign-in lands
    // and the person is watching; every five after, because the rest of the window is somebody
    // reading a code off a phone and polling it 900 times would be asking the service for
    // nothing at fifteen times the rate it can answer differently.
    const QUICK_POLLS = 30;
    const polls = QUICK_POLLS + Math.ceil((29 * 60) / 5);
    let linked: string | null = null;
    let lastError: string | null = null;
    for (let i = 0; i < polls && !linked && mineStill(); i++) {
      await new Promise((r) => setTimeout(r, i < QUICK_POLLS ? 2000 : 5000));
      try {
        linked = await invoke<string | null>("steam_link_status");
        lastError = null;
      } catch (e) {
        // One failed poll is nothing — the service is a network away. A run of them is the
        // reason the wall is still up, so the last one is kept and said out loud below.
        lastError = typeof e === "string" ? e : null;
      }
    }
    if (!mineStill()) return;

    setWaiting(false);
    if (linked) {
      setNote("Signed in. Getting you in…");
      await invoke("recheck_gate").catch(() => {});
    } else if (lastError) {
      setNote(`${lastError}. Try again.`);
    } else {
      // This used to end with "Waiting for Steam to confirm it's you…" still on screen and
      // nothing further ever happening, which is indistinguishable from the app being broken.
      // The browser tab is where the answer is, and it is the half that can fail on its own —
      // so say so, rather than going quiet.
      setNote(
        "Steam hasn't come back. If no browser opened, use the link below. If a tab did open " +
          "and says the sign-in expired or couldn't be confirmed, start it again here.",
      );
    }
  };

  if (!required) return null;

  return (
    // `pointer-events-auto` is load-bearing, not decoration.
    //
    // A Radix dialog opened with `modal` (the default) sets `pointer-events: none` on
    // `document.body` and hands pointer events back only inside its own content. This wall is a
    // sibling of those dialogs, not a child, so while one is open every click on it lands on
    // nothing — and because the page is `z-[100]` and a dialog is `z-50`, the page is still the
    // thing being painted. The button looks completely ordinary, is not disabled, and does not
    // respond: "it won't let me click the sign in with Steam".
    //
    // `LooseSwapPrompt` is the one that makes this routine rather than rare — it opens itself
    // at launch whenever it finds a loose model-swap folder, which is exactly when the page is
    // going up — but any of the twenty-odd dialogs in the app does it. An explicit
    // `pointer-events: auto` on a descendant overrides the `none` it inherits from body, which
    // is the same mechanism Radix uses to re-enable its own content.
    <div className="pointer-events-auto fixed inset-0 z-[100] overflow-y-auto bg-background/90 backdrop-blur-xl">
      <main className="mx-auto grid min-h-full w-full max-w-[1280px] lg:grid-cols-[minmax(0,1fr)_440px]">
        <section className="flex min-h-[380px] flex-col px-8 py-8 sm:px-12 sm:py-10 lg:min-h-full lg:px-16 lg:py-12">
          <div className="flex select-none items-center" data-tauri-drag-region>
            <span className="flex items-baseline gap-2 font-cond">
              <span className="text-sm font-extrabold tracking-[-0.04em] text-foreground">
                MXB App
              </span>
              <span className="text-[11px] font-medium text-faint">
                by <span className="font-semibold text-muted-foreground">mxbsecure</span>
              </span>
            </span>
          </div>

          <div className="my-auto max-w-[650px] py-14 lg:py-20">
            <p className="font-cond text-xs font-semibold uppercase tracking-[0.18em] text-primary">
              Hello, rider.
            </p>
            <h1 className="mt-4 max-w-[620px] text-[clamp(2.6rem,5vw,5rem)] font-extrabold leading-[0.96] tracking-[-0.045em] text-foreground">
              Your MX Bikes life, all in one place.
            </h1>
            <p className="mt-7 max-w-[570px] text-[15px] leading-7 text-muted-foreground sm:text-base">
              MXB App makes it easier to discover and install mods, keep your library organized,
              find a server, and use creator-protected content you own.
            </p>

            <div className="mt-10 grid max-w-[600px] gap-5 border-l border-primary-line pl-5 sm:grid-cols-3 sm:gap-7">
              <div>
                <p className="text-sm font-semibold text-foreground">Find what you want</p>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                  Browse tracks, bikes, gear, and more.
                </p>
              </div>
              <div>
                <p className="text-sm font-semibold text-foreground">Install it simply</p>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                  Skip the folder hunting and manual setup.
                </p>
              </div>
              <div>
                <p className="text-sm font-semibold text-foreground">Ride with confidence</p>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                  Your library and ownership stay connected.
                </p>
              </div>
            </div>
          </div>
        </section>

        <section className="flex items-center border-t border-border bg-window/80 px-8 py-12 backdrop-blur-2xl sm:px-12 lg:border-l lg:border-t-0 lg:px-14">
          <div className="w-full">
            <p className="font-cond text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
              Let’s get you set up
            </p>
            <h2 className="mt-3 text-3xl font-bold tracking-[-0.025em] text-foreground">
              Sign in with Steam
            </h2>
            <p className="mt-4 text-sm leading-6 text-muted-foreground">
              Use the Steam account you play MX Bikes with. We’ll open Steam in your browser and
              bring you back when you’re done.
            </p>

            <Button
              size="lg"
              className="mt-8 w-full"
              disabled={opening}
              onClick={() => void signIn()}
            >
              {opening ? "Opening Steam…" : waiting ? "Open Steam again" : "Continue with Steam"}
            </Button>

            {note && (
              <p className="mt-4 text-sm leading-5 text-muted-foreground" aria-live="polite">
                {note}
              </p>
            )}

            {url && (
              // The escape hatch from a browser that never opened. `openUrl` cannot tell us that
              // happened, so the rider is given the URL itself rather than a reassurance.
              <div className="mt-6 border-t border-border pt-5">
                <p className="text-xs text-muted-foreground">
                  No browser? Copy this link and open it yourself:
                </p>
                <p className="mt-2 max-h-14 overflow-hidden break-all font-mono text-[11px] leading-5 text-muted-foreground/80">
                  {url}
                </p>
                <button
                  type="button"
                  className="mt-3 text-xs font-semibold text-primary underline-offset-4 hover:underline focus-visible:underline"
                  onClick={() => {
                    void navigator.clipboard
                      .writeText(url)
                      .then(() => setCopied(true))
                      .catch(() => setCopied(false));
                  }}
                >
                  {copied ? "Copied" : "Copy sign-in link"}
                </button>
              </div>
            )}

            <p className="mt-8 border-t border-border pt-5 text-xs leading-5 text-muted-foreground/70">
              {waiting
                ? "Finish in your browser. If the tab expired, open Steam again for a fresh link."
                : "You’ll return here automatically after Steam confirms your account."}
            </p>
          </div>
        </section>
      </main>
    </div>
  );
}
