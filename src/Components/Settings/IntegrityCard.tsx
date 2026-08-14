import { useCallback, useEffect, useState } from "react";
import { RefreshCw, ShieldAlert, ShieldCheck, ShieldQuestion, ShieldX } from "lucide-react";
import { toast } from "sonner";
import { useI18n } from "../../i18n/context";
import type { TKey } from "../../i18n";
import { Button } from "@/Components/ui/button";
import { cn } from "@/lib/utils";
import { integrityScanNow, integrityStatus, onIntegrityReport } from "@/api/mods";
import type { IntegrityFinding, IntegrityReport, IntegrityVerdict } from "@/types";

/**
 * What the cheat scan currently sees, and the button that re-runs it.
 *
 * The hardest thing to get right here isn't the layout, it's the wording. Three of the four
 * verdicts must not read as an accusation:
 *
 *   * `unknown` — we couldn't look. Shown as plainly as possible, because a user who reads
 *     it as "fine" has been misled by the app.
 *   * `clean` — nothing unaccounted for. Says what was scanned, so it doesn't read as a
 *     guarantee.
 *   * `suspect` — something is loaded that we don't recognise. This is the one that will be
 *     seen most by people running nothing worse than an overlay we haven't listed, so it is
 *     phrased as a question and paired with what to do about it.
 *   * `flagged` — it matched the rule list. Only this one is allowed to sound like an
 *     accusation.
 */
export default function IntegrityCard({ watching }: { watching: boolean }) {
  const { t } = useI18n();
  const [report, setReport] = useState<IntegrityReport | null>(null);
  const [scanning, setScanning] = useState(false);

  useEffect(() => {
    let live = true;
    void integrityStatus().then((r) => live && setReport(r));
    // The watcher pushes on change, so an open Settings page follows a session rather than
    // showing whatever was true when it was opened.
    const un = onIntegrityReport((r) => live && setReport(r));
    return () => {
      live = false;
      void un.then((f) => f());
    };
  }, []);

  const rescan = useCallback(async () => {
    setScanning(true);
    try {
      setReport(await integrityScanNow());
    } catch (e) {
      toast.error(t("integrity.scanFailed"), { description: String(e) });
    }
    setScanning(false);
  }, [t]);

  if (!watching) {
    return (
      <p className="text-[12px] leading-relaxed text-muted-foreground">
        {t("integrity.offNote")}
      </p>
    );
  }

  const verdict = report?.verdict ?? "unknown";
  const findings = report?.findings ?? [];

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-start gap-3 rounded-lg border border-white/[0.07] px-3 py-2.5">
        <VerdictIcon verdict={verdict} />
        <div className="min-w-0 flex-1">
          <div className="text-[12.5px] text-foreground/90">{t(headline(verdict))}</div>
          <div className="mt-0.5 text-[11.5px] leading-relaxed text-muted-foreground">
            {verdict === "clean"
              ? t("integrity.cleanDetail", { count: report?.modulesScanned ?? 0 })
              : verdict === "unknown"
                ? t(report?.gameRunning ? "integrity.unknownRunning" : "integrity.unknownIdle")
                : t("integrity.foundDetail", { count: findings.length })}
          </div>
        </div>
        <Button size="sm" variant="ghost" onClick={() => void rescan()} disabled={scanning}>
          <RefreshCw className={cn("size-3.5", scanning && "animate-spin")} />
          {t("integrity.scanNow")}
        </Button>
      </div>

      {findings.length > 0 && (
        <ul className="flex flex-col gap-1.5">
          {findings.map((f) => (
            <FindingRow key={`${f.name}:${f.path}`} finding={f} />
          ))}
        </ul>
      )}

      {/* The limit, stated where somebody about to trust this will read it. Leaving it to
          the docs would let the card imply something it cannot deliver. */}
      <p className="text-[11.5px] leading-relaxed text-faint">
        {t("integrity.limitNote")}
        {report && report.rulesVersion > 0 && (
          <> {t("integrity.rulesVersion", { version: report.rulesVersion })}</>
        )}
      </p>
    </div>
  );
}

function VerdictIcon({ verdict }: { verdict: IntegrityVerdict }) {
  const cls = "mt-0.5 size-4 flex-none";
  if (verdict === "clean") return <ShieldCheck className={cn(cls, "text-success")} />;
  if (verdict === "suspect") return <ShieldAlert className={cn(cls, "text-warning")} />;
  if (verdict === "flagged") return <ShieldX className={cn(cls, "text-destructive")} />;
  return <ShieldQuestion className={cn(cls, "text-muted-foreground")} />;
}

function headline(verdict: IntegrityVerdict): TKey {
  switch (verdict) {
    case "clean":
      return "integrity.clean";
    case "suspect":
      return "integrity.suspect";
    case "flagged":
      return "integrity.flagged";
    default:
      return "integrity.unknown";
  }
}

/** Why one thing was reported. A key rather than prose, so it survives six languages. */
function reasonKey(finding: IntegrityFinding): TKey {
  switch (finding.reason) {
    case "ruleName":
    case "ruleHash":
      return "integrity.reasonRule";
    case "loaderHijack":
      return "integrity.reasonLoader";
    default:
      return finding.kind === "process"
        ? "integrity.reasonProcess"
        : "integrity.reasonForeign";
  }
}

function FindingRow({ finding }: { finding: IntegrityFinding }) {
  const { t } = useI18n();
  const flagged = finding.severity === "flagged";
  return (
    <li
      className={cn(
        "rounded-lg border px-3 py-2 text-[12px]",
        flagged
          ? "border-destructive/30 bg-destructive/[0.07]"
          : "border-white/[0.07] bg-white/[0.02]",
      )}
    >
      <div className="flex items-baseline justify-between gap-3">
        <span className="truncate font-medium">{finding.name}</span>
        {/* The rule's own label, never translated — a cheat's name is its name. */}
        {finding.label && (
          <span className="flex-none text-[11.5px] text-destructive">{finding.label}</span>
        )}
      </div>
      <div className="mt-0.5 text-[11.5px] text-muted-foreground">{t(reasonKey(finding))}</div>
      {finding.path && (
        <div className="mt-0.5 truncate text-[11px] text-faint" title={finding.path}>
          {finding.path}
        </div>
      )}
    </li>
  );
}
