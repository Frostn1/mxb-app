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
 */
export default function SigninGate() {
  const [required, setRequired] = useState(false);
  const [message, setMessage] = useState("Sign in with Steam to use MXB App.");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const polling = useRef(false);

  useEffect(() => {
    const pending = listen<{ required: boolean; message: string }>("mxb-signin-required", (e) => {
      setRequired(e.payload.required);
      if (e.payload.message) setMessage(e.payload.message);
      if (!e.payload.required) {
        setBusy(false);
        setNote(null);
      }
    });
    return () => {
      void pending.then((off) => off()).catch(() => {});
    };
  }, []);

  const signIn = async () => {
    setBusy(true);
    setNote("Opening Steam in your browser…");
    try {
      const url = await invoke<string>("steam_link_start");
      await openUrl(url);
      setNote("Waiting for Steam to confirm it's you…");
      // Poll until the link lands, then let the gate have the final word.
      if (polling.current) return;
      polling.current = true;
      for (let i = 0; i < 150; i++) {
        await new Promise((r) => setTimeout(r, 2000));
        let linked: string | null = null;
        try {
          linked = await invoke<string | null>("steam_link_status");
        } catch {
          /* transient — keep waiting */
        }
        if (linked) {
          setNote("Signed in. Getting you in…");
          await invoke("recheck_gate").catch(() => {});
          break;
        }
      }
      polling.current = false;
      setBusy(false);
    } catch (e) {
      polling.current = false;
      setBusy(false);
      setNote(typeof e === "string" ? e : "Couldn't start the sign-in. Try again.");
    }
  };

  if (!required) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-background/95 backdrop-blur-sm">
      <div className="mx-4 w-full max-w-md rounded-2xl border border-border bg-card p-8 text-center shadow-xl">
        <h1 className="text-2xl font-bold tracking-tight text-foreground">Sign in with Steam to continue</h1>
        <p className="mt-3 text-sm text-muted-foreground">{message}</p>
        <p className="mt-1 text-sm text-muted-foreground">It takes one click and keeps your account yours.</p>
        <div className="mt-7">
          <Button size="lg" className="rounded-full px-8" disabled={busy} onClick={() => void signIn()}>
            {busy ? "Signing in…" : "Sign in with Steam"}
          </Button>
        </div>
        {note && <p className="mt-4 text-xs text-muted-foreground">{note}</p>}
        <p className="mt-6 text-xs text-muted-foreground/70">
          Opens Steam in your browser · nothing else works until you do
        </p>
      </div>
    </div>
  );
}
