import { useEffect, useState } from "react";
import { APP_NAME } from "@/i18n";
import { Button } from "@frost/shared/Components/ui/button";
import { appPlatform } from "@frost/shared/api/mods";

/**
 * The studio's shell, deliberately almost empty.
 *
 * What it is for right now is proving the seam: this window is a second binary, built from
 * the same `mxb-core` and the same `@frost/shared` as the manager, and the button below
 * calls a command registered by path across a crate boundary. The tools themselves arrive
 * once that is known to hold.
 */
export default function App() {
  const [platform, setPlatform] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    appPlatform()
      .then((p) => setPlatform(String(p)))
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-6 bg-background p-10 text-foreground">
      <div className="flex select-none items-center">
        <span className="u-skew grid h-[26px] place-items-center bg-primary px-2">
          <span className="u-unskew font-cond text-[16px] font-bold leading-none tracking-[0.04em] text-primary-foreground">
            FROST
          </span>
        </span>
        <span className="ml-[9px] font-cond text-[16px] font-semibold leading-none tracking-[0.08em] text-muted-foreground">
          Studio
        </span>
      </div>

      <p className="max-w-md text-center text-[13px] leading-relaxed text-muted-foreground">
        {APP_NAME} is where paints, tracks and rider kit are made. The tools are still in
        the mod manager; this window is the shell they move into.
      </p>

      <p className="text-[12px] text-muted-foreground">
        {error ? `backend unreachable: ${error}` : `backend says: ${platform ?? "…"}`}
      </p>

      <Button variant="outline" onClick={() => setPlatform(null)}>
        A shared button, from @frost/shared
      </Button>
    </main>
  );
}
