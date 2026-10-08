import { useEffect, useRef, useState } from "react";
import { errorText, type ServerView } from "@/lib/api";
import { claimCode, hostedClaim } from "@/lib/hosted";
import { Button, ErrorLine, Field, Input } from "./ui";

/** Add a server hosted by mxbsecure with the code from servers.mxbsecure.com. A code from an
 *  `mxbservers://` link arrives as `code` and is claimed straight away. */
export function HostedAdd({ code: linked, onSaved, onCancel }: { code?: string; onSaved: (server: ServerView) => void; onCancel: () => void }) {
  const [code, setCode] = useState(linked ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const started = useRef<string | null>(null);

  const claim = async (value: string) => {
    setBusy(true);
    setError(null);
    try {
      onSaved(await hostedClaim(claimCode(value)));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (linked && started.current !== linked) {
      started.current = linked;
      setCode(linked);
      void claim(linked);
    }
    // `claim` is stable enough for a one-shot per code.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [linked]);

  return (
    <form
      className="flex max-w-xl flex-col gap-6"
      onSubmit={(e) => {
        e.preventDefault();
        void claim(code);
      }}
    >
      <div className="flex flex-col gap-1">
        <h2 className="font-heading text-xl font-extrabold tracking-tight">Add hosted server</h2>
        <p className="text-sm text-muted-foreground">Paste the code from servers.mxbsecure.com.</p>
      </div>
      <Field label="Code">
        <Input value={code} onChange={(e) => setCode(e.target.value)} autoFocus spellCheck={false} autoComplete="off" className="font-mono" required />
      </Field>
      {error && <ErrorLine text={error} />}
      <div className="flex gap-2">
        <Button type="submit" variant="primary" disabled={busy || !code.trim()}>
          {busy ? "Adding…" : "Add"}
        </Button>
        <Button type="button" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
