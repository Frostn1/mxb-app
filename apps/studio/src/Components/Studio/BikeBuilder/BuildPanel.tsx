import { useEffect, useState } from "react";
import { FolderOpen, Hammer, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { revealInExplorer } from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import { buildBike, setBuildName, type AssemblyView, type BuildReport } from "../../../api/bikebuild";

/**
 * The bottom strip: name the build and make it. Its own panel, full width under the
 * tray/slots/preview row — a build is the one step that isn't tray, slots or preview, and
 * giving it a fourth column would starve all three of theirs for a name field and a button.
 */
export default function BuildPanel({ view, placedCount }: { view: AssemblyView | null; placedCount: number }) {
  const t = useT();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<BuildReport | null>(null);

  useEffect(() => {
    if (view) setName(view.name);
  }, [view]);

  async function onBuild() {
    setBusy(true);
    await setBuildName(name).catch(() => undefined);
    try {
      setReport(await buildBike());
    } catch (e) {
      toast.error(t("bike.buildFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="flex flex-col gap-3 border-t border-border bg-card p-4">
      <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("bike.build")}</h2>
      <div className="flex flex-wrap items-center gap-2">
        <input
          className="h-8 w-64 border border-input bg-transparent px-2 text-sm"
          placeholder={t("bike.buildNamePlaceholder")}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <Button size="sm" onClick={onBuild} disabled={busy || placedCount === 0}>
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Hammer className="size-3.5" />}
          {t("bike.buildIt")}
        </Button>
      </div>
      {report && (
        <div className="flex flex-col gap-1.5 text-sm">
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
        </div>
      )}
    </section>
  );
}
