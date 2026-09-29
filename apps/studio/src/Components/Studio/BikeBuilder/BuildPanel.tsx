import { useEffect, useRef, useState } from "react";
import { FolderOpen, Hammer, Info, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@frost/shared/Components/ui/popover";
import { revealInExplorer } from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import { buildBike, setBuildName, type AssemblyView, type BuildReport } from "../../../api/bikebuild";

/**
 * Name the build and make it — a small floating pill in the bottom-right corner, not a strip
 * of its own across the full width. It used to be a footer under the tray/preview/outliner
 * row, which meant the build (the last thing a rider does) held a whole row hostage even
 * though naming a bike and clicking one button never needed more than a few inches of it.
 * The report from a finished build is a popover off an info button, open right after building
 * and reopenable any time after, rather than always expanded underneath.
 */
export default function BuildPanel({ view, placedCount }: { view: AssemblyView | null; placedCount: number }) {
  const t = useT();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<BuildReport | null>(null);
  const [reportOpen, setReportOpen] = useState(false);
  /** What we last set the field to from the backend, so a rider typing a name doesn't have
   *  it overwritten every time *anything* elsewhere in the assembly changes — adding a part
   *  or nudging one also bumps `version` and re-reads the assembly, and the name used to
   *  reset to the saved one on every one of those, not just the first load. */
  const lastSynced = useRef<string | null>(null);
  useEffect(() => {
    if (!view) return;
    setName((current) => (lastSynced.current === null || current === lastSynced.current ? view.name : current));
    lastSynced.current = view.name;
  }, [view]);

  async function onBuild() {
    setBusy(true);
    await setBuildName(name).catch(() => undefined);
    try {
      setReport(await buildBike());
      setReportOpen(true);
    } catch (e) {
      toast.error(t("bike.buildFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="pointer-events-auto absolute bottom-3 right-3 z-20 flex items-center gap-1.5 rounded-md border border-border bg-card/95 px-2 py-1.5 shadow-lg backdrop-blur">
      <input
        className="h-7 w-36 border border-input bg-transparent px-2 text-[12px]"
        placeholder={t("bike.buildNamePlaceholder")}
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      <Button size="sm" onClick={onBuild} disabled={busy || placedCount === 0}>
        {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Hammer className="size-3.5" />}
        {t("bike.buildIt")}
      </Button>
      {report && (
        <Popover open={reportOpen} onOpenChange={setReportOpen}>
          <PopoverTrigger asChild>
            <Button size="sm" variant="ghost" title={t("bike.buildReport")}>
              <Info className="size-3.5" />
            </Button>
          </PopoverTrigger>
          <PopoverContent align="end" className="flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-1.5 text-sm">
            <p className="flex items-center gap-2">
              <span className="truncate font-mono text-[11px] text-muted-foreground">{report.folder}</span>
              <Button size="sm" variant="ghost" onClick={() => revealInExplorer(report.folder)}>
                <FolderOpen className="size-3.5" />
                {t("bike.openFolder")}
              </Button>
            </p>
            <p className="font-mono text-[11px]">{report.files.join("  ")}</p>
            <p className="text-[12px] text-muted-foreground">
              {Object.entries(report.tris)
                .map(([g, n]) => `${g} ${n.toLocaleString()} (${t("bike.shadow")} ${report.shadowTris[g] ?? 0})`)
                .join(" · ")}
            </p>
            <p className={report.converted ? "text-[12px] text-emerald-500" : "text-[12px] text-amber-500"}>
              {report.converter}
            </p>
            {report.notes.map((n) => (
              <p key={n} className="text-[12px] text-muted-foreground">
                {n}
              </p>
            ))}
          </PopoverContent>
        </Popover>
      )}
    </section>
  );
}
