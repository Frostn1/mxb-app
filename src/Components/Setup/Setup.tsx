import { useState } from "react";
import { Snowflake, FolderOpen } from "lucide-react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { createConfig } from "../../api/mods";
import { Button } from "@/Components/ui/button";

interface SetupProps {
  onComplete: () => void;
}

export default function Setup({ onComplete }: SetupProps) {
  const [chosen, setChosen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const finish = async (modsPath: string) => {
    setBusy(true);
    setError(null);
    try {
      await createConfig({ modsPath });
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
              <span className="font-mono text-foreground/80">
                Documents\PiBoSo\MX Bikes
              </span>{" "}
              folder. You can also pick it yourself.
            </p>
          )}
          <button
            onClick={choose}
            className="cursor-default self-start text-[12px] font-semibold text-primary hover:brightness-110"
          >
            {chosen ? "Choose a different folder…" : "Choose the folder manually…"}
          </button>
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
