import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Button } from "../ui/button";

/**
 * The Steam sign-in wall — shared by every app in the lineup.
 *
 * When the deployment requires a Valve-confirmed Steam sign-in (`MXB_REQUIRE_STEAM` on the
 * control plane), the startup gate answers `signin` and the Rust side emits
 * `mxb-signin-required`. This overlay covers the whole window on that event and nothing behind
 * it can be used until the account is linked — the enforcement is the gate, this is the door.
 *
 * The flow reuses the commands the secure-content link already had: `steam_link_start` returns a
 * Steam OpenID URL, the browser half lands on the control plane and sets the Steam id, and
 * `steam_link_status` reports when it has. Once linked we call `recheck_gate`, which re-asks the
 * gate; a clean verdict emits `mxb-signin-required { required: false }` and the wall comes down.
 *
 * Honest throughout: this is a requirement to meet, not the disguised block a banned install
 * gets (that one closes the app instead of ever reaching here).
 *
 * The one rule this door has to keep: **the button is never dead**. It is the only control on
 * the only thing on screen, so a state it cannot be clicked out of is the app not opening. It
 * used to be disabled for the whole wait — minutes, on a sign-in that had already failed in the
 * browser, with no way to start another and no way to tell that from the app having hung. Now it
 * only goes quiet for the moment it takes to open the browser, and every wait can be restarted
 * over the top of itself.
 */
export default function SigninGate() {
  const [required, setRequired] = useState(false);
  const [message, setMessage] = useState("Sign in with Steam to use MXB App.");
  /** The browser is being opened. The one moment when a second click has nothing to do. */
  const [opening, setOpening] = useState(false);
  /** The browser is open and we are waiting on Steam. Clicking again starts a fresh sign-in. */
  const [waiting, setWaiting] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  /**
   * Which attempt is the live one. Bumped by every click, so a poll left over from an earlier
   * attempt sees it has been superseded and stops touching the screen — the alternative being an
   * old loop's "Steam hasn't come back" landing on top of the attempt the person is watching.
   */
  const attempt = useRef(0);

  useEffect(() => {
    const pending = listen<{ required: boolean; message: string }>("mxb-signin-required", (e) => {
      setRequired(e.payload.required);
      if (e.payload.message) setMessage(e.payload.message);
      if (!e.payload.required) {
        attempt.current++;
        setOpening(false);
        setWaiting(false);
        setNote(null);
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

    try {
      const url = await invoke<string>("steam_link_start");
      await openUrl(url);
    } catch (e) {
      if (!mineStill()) return;
      setOpening(false);
      setNote(typeof e === "string" ? e : "Couldn't start the sign-in. Try again.");
      return;
    }
    if (!mineStill()) return;

    // The browser is open, so the button comes back: from here the useful thing a second click
    // does is open Steam again, which is exactly what somebody looking at a browser tab that
    // said "sign-in expired" needs.
    setOpening(false);
    setWaiting(true);
    setNote("Waiting for Steam to confirm it's you…");

    // Poll until the link lands, then let the gate have the final word. The ceiling matches the
    // control plane's own ten-minute sign-in window: stopping at five left a sign-in that was
    // still perfectly valid — a Steam Guard prompt, a password typed slowly — with nothing
    // watching for it.
    let linked: string | null = null;
    let lastError: string | null = null;
    for (let i = 0; i < 300 && !linked && mineStill(); i++) {
      await new Promise((r) => setTimeout(r, 2000));
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
        "Steam hasn't come back. Check the browser tab that opened: if it says the sign-in " +
          "expired or couldn't be confirmed, start it again here.",
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
    // nothing — and because the wall is `z-[100]` and a dialog is `z-50`, the wall is still the
    // thing being painted. The button looks completely ordinary, is not disabled, and does not
    // respond: "it won't let me click the sign in with Steam".
    //
    // `LooseSwapPrompt` is the one that makes this routine rather than rare — it opens itself
    // at launch whenever it finds a loose model-swap folder, which is exactly when the wall is
    // going up — but any of the twenty-odd dialogs in the app does it. An explicit
    // `pointer-events: auto` on a descendant overrides the `none` it inherits from body, which
    // is the same mechanism Radix uses to re-enable its own content.
    <div className="pointer-events-auto fixed inset-0 z-[100] flex items-center justify-center bg-background/95 backdrop-blur-sm">
      <div className="mx-4 w-full max-w-md rounded-2xl border border-border bg-card p-8 text-center shadow-xl">
        <h1 className="text-2xl font-bold tracking-tight text-foreground">Sign in with Steam to continue</h1>
        <p className="mt-3 text-sm text-muted-foreground">{message}</p>
        <p className="mt-1 text-sm text-muted-foreground">It takes one click and keeps your account yours.</p>
        <div className="mt-7">
          <Button size="lg" className="rounded-full px-8" disabled={opening} onClick={() => void signIn()}>
            {opening ? "Opening Steam…" : waiting ? "Open Steam again" : "Sign in with Steam"}
          </Button>
        </div>
        {note && <p className="mt-4 text-xs text-muted-foreground">{note}</p>}
        <p className="mt-6 text-xs text-muted-foreground/70">
          {waiting
            ? "Finish the sign-in in your browser · press the button again for a fresh one"
            : "Opens Steam in your browser · nothing else works until you do"}
        </p>
      </div>
    </div>
  );
}
