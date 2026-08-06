import { useEffect, useState } from "react";
import { Snowflake, FolderOpen, Gamepad2, Loader2, Check } from "lucide-react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { createConfig, detectGamePath } from "../../api/mods";
import { usePlatform } from "../../lib/usePlatform";
import { Button } from "@/Components/ui/button";

interface SetupProps {
  onComplete: () => void;
}

/**
 * Where MX Bikes keeps its user folder, per OS. On Linux the game runs under Proton and
 * writes inside the Wine prefix, so pointing someone at `~/Documents` would send them
 * looking in a folder MX Bikes has never touched.
 */
function hintFor(platform: string | null): string {
  if (platform === "linux") return "steamapps/compatdata/655500/.../Documents/PiBoSo/MX Bikes";
  if (platform === "macos") return "Documents/PiBoSo/MX Bikes";
  return "Documents\\PiBoSo\\MX Bikes";
}

export default function Setup({ onComplete }: SetupProps) {
  const defaultHint = hintFor(usePlatform());
  const [chosen, setChosen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // MX Bikes install (Steam) folder — auto-detected on mount so the 3D rider
  // preview works out of the box, with a manual fallback when detection misses.
  const [detecting, setDetecting] = useState(true);
  const [gamePath, setGamePath] = useState<string | null>(null);
  const [gameAuto, setGameAuto] = useState(false);

  useEffect(() => {
    let cancelled = false;
    detectGamePath()
      .then((found) => {
        if (cancelled) return;
        if (found) {
          setGamePath(found);
          setGameAuto(true);
        }
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setDetecting(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const finish = async (modsPath: string) => {
    setBusy(true);
    setError(null);
    try {
      await createConfig({ modsPath, gamePath: gamePath ?? "" });
      onComplete();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const choose = async () => {
    const picked = await pickFolder({
      directory: true,
      multiple: false,
      title: "Select your MX Bikes folder",
    });
    if (typeof picked === "string") setChosen(picked);
  };

  const chooseGame = async () => {
    const picked = await pickFolder({
      directory: true,
      multiple: false,
      title: "Select your MX Bikes install folder",
    });
    if (typeof picked === "string") {
      setGamePath(picked);
      setGameAuto(false);
    }
  };

  return (
    <div className="grid min-h-0 flex-1 place-items-center px-10">
      <div className="flex w-full max-w-[480px] flex-col items-center gap-7 pb-16">
        <div className="flex flex-col items-center gap-3.5">
          <div className="grid size-14 place-items-center rounded-[15px] bg-gradient-to-br from-[#9ccfec] to-[#5d8fb0] text-[#0d0f12]">
            <Snowflake className="size-7" strokeWidth={2.5} />
          </div>
          <div className="flex flex-col items-center gap-1.5">
            <h1 className="text-[26px] font-extrabold tracking-[-0.4px]">
              Welcome to MXB App
            </h1>
            <p className="max-w-[380px] text-center text-[13.5px] leading-relaxed text-muted-foreground">
              Browse mxb-mods, install with one click, and let FrostMod reload the
              game for you.
            </p>
          </div>
        </div>

        <div className="flex w-full flex-col gap-2.5">
          <span className="text-[11.5px] font-bold uppercase tracking-[1px] text-faint">
            MX Bikes folder
          </span>
          {chosen ? (
            <div className="flex items-center gap-2.5 rounded-[10px] border border-input bg-card px-3.5 py-3 font-mono text-[12.5px] text-muted-foreground">
              <FolderOpen className="size-4 flex-none text-primary" />
              <span className="flex-1 truncate" title={chosen}>
                {chosen}
              </span>
            </div>
          ) : (
            <p className="text-[12.5px] text-muted-foreground">
              MXB App will auto-detect your{" "}
              <span className="font-mono text-foreground/80">{defaultHint}</span> folder.
              You can also pick it yourself.
            </p>
          )}
          <button
            onClick={choose}
            className="cursor-default self-start text-[12px] font-semibold text-primary hover:brightness-110"
          >
            {chosen ? "Choose a different folder…" : "Choose the folder manually…"}
          </button>
        </div>

        <div className="flex w-full flex-col gap-2.5">
          <span className="text-[11.5px] font-bold uppercase tracking-[1px] text-faint">
            MX Bikes install
          </span>
          {detecting ? (
            <div className="flex items-center gap-2.5 rounded-[10px] border border-input bg-card px-3.5 py-3 text-[12.5px] text-muted-foreground">
              <Loader2 className="size-4 flex-none animate-spin text-primary" />
              <span>Looking for your MX Bikes install…</span>
            </div>
          ) : gamePath ? (
            <>
              <div className="flex items-center gap-2.5 rounded-[10px] border border-input bg-card px-3.5 py-3 font-mono text-[12.5px] text-muted-foreground">
                <Gamepad2 className="size-4 flex-none text-primary" />
                <span className="flex-1 truncate" title={gamePath}>
                  {gamePath}
                </span>
                {gameAuto && (
                  <span
                    className="flex flex-none items-center gap-1 font-sans text-[11px] font-semibold text-primary"
                    title="Detected automatically"
                  >
                    <Check className="size-3.5" strokeWidth={2.5} />
                    Found
                  </span>
                )}
              </div>
              <button
                onClick={chooseGame}
                className="cursor-default self-start text-[12px] font-semibold text-primary hover:brightness-110"
              >
                Choose a different folder…
              </button>
            </>
          ) : (
            <>
              <p className="text-[12.5px] text-muted-foreground">
                Couldn&apos;t find your MX Bikes install automatically — it powers
                the
                3D rider preview. Pick it manually, or set it later in Settings.
              </p>
              <button
                onClick={chooseGame}
                className="cursor-default self-start text-[12px] font-semibold text-primary hover:brightness-110"
              >
                Choose the install folder manually…
              </button>
            </>
          )}
        </div>

        {error && (
          <p className="w-full select-text text-center text-[12px] text-destructive">
            {error}
          </p>
        )}

        <Button
          className="h-12 w-full text-[14.5px]"
          disabled={busy}
          onClick={() => finish(chosen ?? "")}
        >
          {chosen ? "Start browsing mods" : "Detect & start browsing"}
        </Button>
      </div>
    </div>
  );
}
