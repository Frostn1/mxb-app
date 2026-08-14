import { useCallback, useEffect, useState } from "react";
import { RefreshCw, ShieldAlert, ShieldCheck, ShieldQuestion, ShieldX } from "lucide-react";
import { useI18n } from "../../i18n/context";
import { Button } from "@/Components/ui/button";
import { cn } from "@/lib/utils";
import { integrityServerReports } from "@/api/mods";
import type { IntegrityVerdict, RiderIntegrity } from "@/types";

/** How often to re-read the grid. Clients publish every 45 seconds, so anything faster is
 *  asking the same question twice. */
const POLL_MS = 60_000;

/**
 * What the riders on this server have said about their own clients.
 *
 * The heading and the empty state carry most of the weight here, because the thing an admin
 * will do with this is accuse somebody, and the list does not support that on its own. A
 * rider appears only if their app is running, watching, and set to report — so this answers
 * "who has attested", and the people missing from it are exactly the ones it can say nothing
 * about. A cheater who closed MXB App is not in this list, and that is not a bug.
 */
export default function ServerIntegrity({ serverId }: { serverId: string }) {
  const { t } = useI18n();
  const [riders, setRiders] = useState<RiderIntegrity[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      setRiders(await integrityServerReports(serverId));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  }, [serverId]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  return (
    <div className="mt-3 border-t border-white/[0.05] pt-3">
      <div className="flex items-center gap-2">
        <ShieldCheck className="size-3.5 flex-none text-muted-foreground" />
        <span className="text-[12px] text-muted-foreground">{t("integrity.gridTitle")}</span>
        <Button
          className="ml-auto"
          size="sm"
          variant="ghost"
          onClick={() => void refresh()}
          disabled={busy}
        >
          <RefreshCw className={cn("size-3.5", busy && "animate-spin")} />
        </Button>
      </div>

      {error ? (
        <p className="mt-2 text-[11.5px] text-muted-foreground">{error}</p>
      ) : riders === null ? (
        <p className="mt-2 text-[11.5px] text-muted-foreground">{t("integrity.gridLoading")}</p>
      ) : riders.length === 0 ? (
        <p className="mt-2 text-[11.5px] leading-relaxed text-muted-foreground">
          {t("integrity.gridEmpty")}
        </p>
      ) : (
        <ul className="mt-2 flex flex-col gap-1">
          {riders.map((r) => (
            <RiderRow key={r.guid || r.riderName} rider={r} />
          ))}
        </ul>
      )}

      <p className="mt-2 text-[11px] leading-relaxed text-faint">{t("integrity.gridNote")}</p>
    </div>
  );
}

function RiderRow({ rider }: { rider: RiderIntegrity }) {
  const { t } = useI18n();
  // The worst of the recent past outranks the current answer on the row itself: a client
  // that loaded a cheat for one lap and unloaded it reports clean now, and "clean" is not
  // what an admin should see for it.
  const shown = worse(rider.worstVerdict, rider.verdict);
  const recovered = shown !== rider.verdict;
  return (
    <li className="flex items-center gap-2 rounded-lg border border-white/[0.07] bg-white/[0.02] px-3 py-1.5 text-[12px]">
      <VerdictDot verdict={shown} />
      <span className="min-w-0 flex-1 truncate">{rider.riderName}</span>
      <span
        className={cn(
          "flex-none text-[11.5px]",
          shown === "flagged"
            ? "text-destructive"
            : shown === "suspect"
              ? "text-warning"
              : "text-muted-foreground",
        )}
      >
        {rider.flagged.length > 0
          ? // The rule's own label, verbatim and untranslated.
            rider.flagged.map((f) => f.label || f.name).join(", ")
          : shown === "suspect"
            ? t("integrity.gridSuspect", { count: rider.suspectCount })
            : t(shown === "clean" ? "integrity.clean" : "integrity.unknown")}
        {recovered && ` ${t("integrity.gridRecovered")}`}
      </span>
    </li>
  );
}

function worse(a: IntegrityVerdict | undefined, b: IntegrityVerdict): IntegrityVerdict {
  const order: IntegrityVerdict[] = ["unknown", "clean", "suspect", "flagged"];
  if (!a) return b;
  return order.indexOf(a) > order.indexOf(b) ? a : b;
}

function VerdictDot({ verdict }: { verdict: IntegrityVerdict }) {
  const cls = "size-3.5 flex-none";
  if (verdict === "clean") return <ShieldCheck className={cn(cls, "text-success")} />;
  if (verdict === "suspect") return <ShieldAlert className={cn(cls, "text-warning")} />;
  if (verdict === "flagged") return <ShieldX className={cn(cls, "text-destructive")} />;
  return <ShieldQuestion className={cn(cls, "text-muted-foreground")} />;
}
