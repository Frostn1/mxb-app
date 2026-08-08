import { ArrowRight, ExternalLink, Sparkles } from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Button } from "@/Components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/Components/ui/dialog";
import { useConfig } from "../../Context/Config";
import { useI18n } from "../../i18n/context";
import { prettyHotkey } from "../../lib/hotkey";
import { usePlatform } from "../../lib/usePlatform";
import type { Release } from "./releases";

const RELEASE_NOTES_URL = "https://github.com/Frostn1/mxb-app/releases/tag/v";

interface ReleaseShowcaseProps {
  release: Release;
  /** Close it and remember this version has been seen. */
  onDone: () => void;
  /** Jump to a Settings section — used by the hero's call to action. */
  onOpenSettings: (section: "overlay") => void;
}

/**
 * The "what's new" modal shown once after an update.
 *
 * One headline feature, a line each for the rest, and a link out to the release notes.
 * The full changelog for 0.7.0 runs to forty entries; anything that tried to show them
 * all would be a wall the player closes without reading, which is exactly what the
 * overlay — a feature that is invisible until you know its shortcut — can't afford.
 */
export default function ReleaseShowcase({
  release,
  onDone,
  onOpenSettings,
}: ReleaseShowcaseProps) {
  const { t } = useI18n();
  const { config } = useConfig();
  const isMac = usePlatform() === "macos";
  const HeroIcon = release.hero.icon;

  // The player's own combo, not the shipped default — the whole point of showing it is
  // that pressing it works, and they may already have rebound it.
  const hotkey = prettyHotkey(
    config.overlayHotkey || "CommandOrControl+Shift+X",
    isMac,
  );

  return (
    <Dialog open onOpenChange={(open) => !open && onDone()}>
      <DialogContent
        className="max-w-[520px] gap-5"
        // Closing by clicking away would leave someone unsure whether it counted;
        // Esc and the corner X both land on `onDone`, which does record it.
        onPointerDownOutside={(e) => e.preventDefault()}
      >
        <div className="flex flex-col gap-1.5">
          <span className="flex items-center gap-1.5 text-[11.5px] font-semibold uppercase tracking-wide text-primary">
            <Sparkles className="size-3.5" />
            {t("showcase.eyebrow")}
          </span>
          <DialogTitle className="text-[20px] tracking-[-0.3px]">
            {t("showcase.title", { version: release.version })}
          </DialogTitle>
          <DialogDescription>{t("showcase.subtitle")}</DialogDescription>
        </div>

        {/* The headline feature, given the room the others don't get. */}
        <div className="flex flex-col gap-3 rounded-xl border border-primary/25 bg-primary/[0.07] p-4">
          <div className="flex items-start gap-3">
            <div className="grid size-9 flex-none place-items-center rounded-[10px] bg-gradient-to-br from-[#9ccfec] to-[#5d8fb0] text-[#0d0f12]">
              <HeroIcon className="size-[18px]" strokeWidth={2.5} />
            </div>
            <div className="flex flex-col gap-1">
              <span className="text-[14px] font-bold">
                {t(release.hero.title)}
              </span>
              <p className="text-[12.5px] leading-relaxed text-muted-foreground">
                {t(release.hero.body)}
              </p>
            </div>
          </div>

          {release.hero.showsHotkey && (
            <div className="flex flex-wrap items-center gap-2 pl-12">
              <span className="rounded-lg border border-white/[0.12] bg-background/60 px-2.5 py-1 font-mono text-[12px] text-foreground/85">
                {hotkey}
              </span>
              <span className="text-[11.5px] text-muted-foreground">
                {t("showcase.whileGameRunning")}
              </span>
            </div>
          )}

          {release.hero.settingsAction && (
            <button
              onClick={() => {
                onDone();
                onOpenSettings(release.hero.settingsAction!.section);
              }}
              className="flex cursor-default items-center gap-1 self-start pl-12 text-[12px] font-semibold text-primary hover:brightness-110"
            >
              {t(release.hero.settingsAction.label)}
              <ArrowRight className="size-3.5" />
            </button>
          )}
        </div>

        <ul className="flex flex-col gap-2.5">
          {release.highlights.map((h) => {
            const Icon = h.icon;
            return (
              <li key={h.text} className="flex items-start gap-2.5">
                <Icon className="mt-[1px] size-4 flex-none text-muted-foreground" />
                <span className="text-[12.5px] leading-relaxed text-foreground/85">
                  {t(h.text)}
                </span>
              </li>
            );
          })}
        </ul>

        <div className="flex items-center justify-between gap-3 border-t border-border pt-3">
          <button
            onClick={() => openUrl(`${RELEASE_NOTES_URL}${release.version}`)}
            className="flex cursor-default items-center gap-1 text-[12px] font-semibold text-primary hover:brightness-110"
          >
            {t("showcase.releaseNotes")}
            <ExternalLink className="size-3" />
          </button>
          <Button size="sm" onClick={onDone}>
            {t("showcase.gotIt")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
