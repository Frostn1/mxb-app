import { useEffect, useMemo, useState } from "react";
import { CheckCircle2, Circle, Search } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/Components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
} from "@/Components/ui/dialog";
import { Segmented } from "@/Components/ui/segmented";
import type { ModEntry, Preset, PresetContent } from "../../types";
import { useT } from "../../i18n/context";
import { displayName, formatBytes } from "../../lib/mods";
import { categoryIcon } from "../Library/categories";
import { contentOf, contentType } from "./Manage";

type Pane = "tracks" | "keep";

/**
 * Pick the content a preset's race needs: the track(s) it's ridden on, and the mods to
 * keep enabled alongside them.
 *
 * The two lists are deliberately separate. A track is *what this preset is for* and reads
 * on the row in the panel behind; the pinned list is the boring infrastructure a race
 * needs anyway (the OEM pack, a sound pack) and is set once and forgotten. Everything the
 * loadout itself references — paint, helmet, boots, model swap — resolves automatically at
 * apply time and doesn't belong in either list.
 */
export function ContentDialog({
  preset,
  mods,
  onClose,
  onSave,
}: {
  preset: Preset | null;
  mods: ModEntry[];
  onClose: () => void;
  onSave: (preset: Preset, content: PresetContent) => void;
}) {
  const t = useT();
  const [pane, setPane] = useState<Pane>("tracks");
  const [search, setSearch] = useState("");
  const [tracks, setTracks] = useState<string[]>([]);
  const [keep, setKeep] = useState<string[]>([]);

  // Re-prime whenever a different preset is opened.
  useEffect(() => {
    if (!preset) return;
    const content = contentOf(preset);
    setTracks(content.tracks);
    setKeep(content.keep);
    setPane("tracks");
    setSearch("");
  }, [preset]);

  const selected = pane === "tracks" ? tracks : keep;
  const setSelected = pane === "tracks" ? setTracks : setKeep;

  const shown = useMemo(() => {
    const q = search.trim().toLowerCase();
    return mods
      .filter((m) => (pane === "tracks" ? contentType(m.rel) === "tracks" : true))
      .filter((m) => q === "" || m.name.toLowerCase().includes(q))
      .slice(0, 400);
  }, [mods, pane, search]);

  // A mod named by the preset but no longer installed still has to be visible — otherwise
  // it's silently dropped the next time the content is saved.
  const missing = useMemo(() => {
    const known = new Set(mods.map((m) => m.rel.toLowerCase()));
    return selected.filter((rel) => !known.has(rel.toLowerCase()));
  }, [mods, selected]);

  const toggle = (rel: string) =>
    setSelected((prev) =>
      prev.some((r) => r.toLowerCase() === rel.toLowerCase())
        ? prev.filter((r) => r.toLowerCase() !== rel.toLowerCase())
        : [...prev, rel],
    );

  const isOn = (rel: string) => selected.some((r) => r.toLowerCase() === rel.toLowerCase());

  return (
    <Dialog open={Boolean(preset)} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("manage.contentTitle", { name: preset?.name ?? "" })}</DialogTitle>
          <DialogDescription>{t("manage.contentBody")}</DialogDescription>
        </DialogHeader>

        <div className="flex items-center gap-3">
          <Segmented
            size="sm"
            value={pane}
            onChange={(v) => setPane(v as Pane)}
            options={[
              {
                value: "tracks",
                label: (
                  <span className="flex items-center gap-1.5">
                    {t("manage.paneTracks")}
                    <span className="text-muted-foreground">{tracks.length}</span>
                  </span>
                ),
              },
              {
                value: "keep",
                label: (
                  <span className="flex items-center gap-1.5">
                    {t("manage.paneKeep")}
                    <span className="text-muted-foreground">{keep.length}</span>
                  </span>
                ),
              },
            ]}
          />
          <div className="ml-auto flex w-[220px] items-center gap-2 rounded-lg border border-input bg-card px-3 py-1.5">
            <Search className="size-3.5 text-faint" />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t("library.searchPlaceholder")}
              className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
            />
          </div>
        </div>

        <p className="text-[11.5px] text-muted-foreground">
          {pane === "tracks" ? t("manage.paneTracksHint") : t("manage.paneKeepHint")}
        </p>

        <div className="flex max-h-[46vh] min-h-[220px] flex-col gap-1 overflow-y-auto pr-1">
          {missing.map((rel) => (
            <button
              key={rel}
              onClick={() => toggle(rel)}
              className="flex cursor-default items-center gap-2.5 rounded-lg border border-destructive/40 bg-destructive/[0.07] px-3 py-2 text-left"
            >
              <CheckCircle2 className="size-4 flex-none text-destructive" />
              <span className="min-w-0 truncate text-[12.5px]">
                {displayName(rel.split("/").pop() ?? rel)}
              </span>
              <span className="ml-auto flex-none text-[11px] text-destructive">
                {t("manage.notInstalled")}
              </span>
            </button>
          ))}
          {shown.length === 0 && missing.length === 0 ? (
            <p className="py-12 text-center text-[12.5px] text-muted-foreground">
              {t("library.noMatches")}
            </p>
          ) : (
            shown.map((m) => {
              const Icon = categoryIcon(m.category);
              const on = isOn(m.rel);
              return (
                <button
                  key={m.rel}
                  onClick={() => toggle(m.rel)}
                  className={cn(
                    "flex cursor-default items-center gap-2.5 rounded-lg border px-3 py-2 text-left transition-colors",
                    on
                      ? "border-primary/60 bg-primary/[0.06]"
                      : "border-white/[0.07] hover:border-white/15",
                  )}
                >
                  {on ? (
                    <CheckCircle2 className="size-4 flex-none text-primary" />
                  ) : (
                    <Circle className="size-4 flex-none text-faint" />
                  )}
                  <Icon className="size-3.5 flex-none text-faint" />
                  <span className="min-w-0 truncate text-[12.5px]">
                    {displayName(m.name)}
                  </span>
                  <span className="ml-auto flex-none text-[11px] text-faint">
                    {[m.folder, formatBytes(m.size), m.enabled ? "" : t("manage.off")]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                </button>
              );
            })
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            onClick={() => preset && onSave(preset, { tracks, keep })}
            disabled={!preset}
          >
            {t("common.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
