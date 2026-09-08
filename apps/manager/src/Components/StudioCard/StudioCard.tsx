import { useCallback, useEffect, useState } from "react";
import { ExternalLink, Palette } from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { launchStudio, studioInstall, type StudioInstall } from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { toast } from "sonner";
import { useT } from "@/i18n";

const STUDIO_RELEASES = "https://github.com/Frostn1/frost-studio/releases/latest";

/**
 * Where the Studio tab used to be.
 *
 * The tools moved to their own app, and someone who used them daily should find out here
 * rather than by noticing a missing tab. Shows the way in when it is installed and the way
 * to get it when it is not — and re-checks on focus, because installing it is exactly the
 * thing someone does in another window and then comes back.
 */
export default function StudioCard() {
  const t = useT();
  const [found, setFound] = useState<StudioInstall | null>(null);
  const [checked, setChecked] = useState(false);

  const check = useCallback(() => {
    studioInstall()
      .then(setFound)
      .catch(() => setFound(null))
      .finally(() => setChecked(true));
  }, []);

  useEffect(() => {
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [check]);

  const open = async () => {
    try {
      await launchStudio();
    } catch (e) {
      toast.error(t("studioApp.launchFailed"), { description: String(e) });
    }
  };

  return (
    <div className="flex h-full items-center justify-center p-10">
      <div className="max-w-lg text-center">
        <div className="mx-auto flex size-12 items-center justify-center bg-primary/10">
          <Palette className="size-6 text-primary" />
        </div>
        <h2 className="mt-5 font-cond text-[20px] font-semibold tracking-[0.04em]">
          {t("studioApp.title")}
        </h2>
        <p className="mt-3 text-[13px] leading-relaxed text-muted-foreground">
          {t("studioApp.pitch")}
        </p>

        <div className="mt-6 flex items-center justify-center gap-3">
          {found ? (
            <Button onClick={() => void open()}>{t("studioApp.open")}</Button>
          ) : (
            <Button onClick={() => void openUrl(STUDIO_RELEASES)}>
              <ExternalLink className="size-3.5" />
              {t("studioApp.download")}
            </Button>
          )}
        </div>

        {checked && (
          <p className="mt-4 text-[11.5px] text-faint">
            {found
              ? t("studioApp.installed", { version: found.version || "—" })
              : t("studioApp.notInstalled")}
          </p>
        )}
      </div>
    </div>
  );
}
