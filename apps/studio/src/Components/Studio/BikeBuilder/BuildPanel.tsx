import { forwardRef, useEffect, useRef, useState } from "react";
import { FolderOpen, Hammer, Info, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@frost/shared/Components/ui/popover";
import { revealInExplorer } from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import { buildBike, setBuildName, type AssemblyView, type BuildReport } from "../../../api/bikebuild";
import BlenderNeeded, { type useBlenderGate } from "./BlenderNeeded";
import { Tip } from "./Tip";

/**
 * Name the build and make it: a bar along the bottom of the tab, always there in full. It
 * used to float over the 3D view's corner, where a short window clipped it and the outliner's
 * last rows sat underneath it; as its own `shrink-0` row the panels above shrink first.
 * The report from a finished build is a popover off an info button, open right after
 * building and reopenable any time after.
 */
const BuildPanel = forwardRef<
  HTMLInputElement,
  {
    view: AssemblyView | null;
    placedCount: number;
    /** A build needs a chassis on the bike: the button says so rather than fail. */
    hasChassis: boolean;
    gate: ReturnType<typeof useBlenderGate>;
    /** Left of the bar: a one-line status of the build so far. */
    status?: React.ReactNode;
  }
>(function BuildPanel({ view, placedCount, hasChassis, gate, status }, nameRef) {
  const t = useT();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<BuildReport | null>(null);
  const [reportOpen, setReportOpen] = useState(false);
  /** What we last set the field to from the backend, so a rider typing a name doesn't have
   *  it overwritten every time *anything* elsewhere in the assembly changes. */
  const lastSynced = useRef<string | null>(null);
  useEffect(() => {
    if (!view) return;
    setName((current) => (lastSynced.current === null || current === lastSynced.current ? view.name : current));
    lastSynced.current = view.name;
  }, [view]);

  async function onBuild() {
    if (!gate.need("build")) return;
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
    <section className="flex h-11 shrink-0 items-center gap-2 border-t border-border bg-window px-3">
      <div className="min-w-0 flex-1 truncate text-[11.5px] text-faint">{status}</div>
      <BlenderNeeded gate={gate} at="build" className="max-w-[22rem]" />
      <input
        ref={nameRef}
        className="h-7 w-44 min-w-0 shrink border border-input bg-transparent px-2 text-[12px]"
        placeholder={t("bike.buildNamePlaceholder")}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && placedCount > 0 && hasChassis && !busy) void onBuild();
        }}
      />
      <Tip label={hasChassis ? t("bike.buildTip") : t("bike.buildNeedsChassis")} side="top" className="shrink-0">
        <Button size="sm" className="h-7 shrink-0" onClick={onBuild} disabled={busy || placedCount === 0 || !hasChassis}>
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Hammer className="size-3.5" />}
          {t("bike.buildIt")}
        </Button>
      </Tip>
      {report && (
        <Popover open={reportOpen} onOpenChange={setReportOpen}>
          <PopoverTrigger asChild>
            <Button size="sm" variant="ghost" className="h-7 shrink-0 px-2" title={t("bike.buildReport")}>
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
});

export default BuildPanel;
