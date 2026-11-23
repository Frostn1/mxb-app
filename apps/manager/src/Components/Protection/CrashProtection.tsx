import { open as openUrl } from "@tauri-apps/plugin-shell";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import { crashName, type CrashStatus, useProtection } from "../../lib/protection";
import HostFrostServer from "./HostFrostServer";

const STATUS: Record<CrashStatus, { label: TKey; tone: string }> = {
  fixed: { label: "protection.statusFixed", tone: "text-success" },
  partly: { label: "protection.statusPartly", tone: "text-warning" },
  open: { label: "protection.statusOpen", tone: "text-muted-foreground" },
  outside: { label: "protection.statusOutside", tone: "text-muted-foreground" },
};

function when(t: number): string {
  return new Date(t * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/**
 * The Crash protection page: the coverage figure, this PC's record, and every known crash
 * with its share of the reports, whether FrostMod fixes it and the write-up.
 */
export default function CrashProtection() {
  const t = useT();
  const { caught, table } = useProtection();
  return (
    <div className="flex min-w-0 flex-col gap-5">
      {table && (
        <div className="flex flex-col gap-1">
          <span className="text-[26px] font-bold tabular-nums leading-none">{table.coverage}%</span>
          <span className="text-[12.5px]">{t("protection.coverage")}</span>
          <span className="text-[11.5px] text-muted-foreground">
            {t("protection.coverageFrom", {
              reports: table.reports.toLocaleString(),
              riders: table.riders.toLocaleString(),
              from: table.from,
              to: table.to,
            })}
          </span>
        </div>
      )}

      <div className="flex flex-col">
        <div className="flex items-baseline gap-2 border-b border-border pb-1.5">
          <span className="flex-1 text-[13px] font-semibold">{t("protection.yours")}</span>
          <span className="text-[12px] tabular-nums text-muted-foreground">
            {t("protection.yoursTotal", { count: caught.length })}
          </span>
        </div>
        {caught.length === 0 ? (
          <p className="py-2 text-[12px] text-muted-foreground">{t("protection.yoursEmpty")}</p>
        ) : (
          <ul className="max-h-[220px] overflow-y-auto">
            {caught.slice(0, 200).map((c, i) => (
              <li
                key={`${c.t}-${c.kind}-${i}`}
                className="flex items-baseline gap-3 border-b border-border/60 py-1.5 text-[12px]"
              >
                <span className="w-[112px] shrink-0 tabular-nums text-muted-foreground">{when(c.t)}</span>
                <span className="min-w-0 flex-1 truncate">{crashName(table, c.kind)}</span>
                <span className="max-w-[40%] shrink-0 truncate text-muted-foreground">
                  {c.track || t("protection.menus")}
                  {c.server ? ` · ${c.server}` : ""}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      {table && (
        <div className="flex flex-col">
          <div className="flex items-baseline gap-3 border-b border-border pb-1.5 text-[11.5px] text-muted-foreground">
            <span className="flex-1 text-[13px] font-semibold text-foreground">{t("protection.known")}</span>
            <span className="w-[64px] text-right">{t("protection.colShare")}</span>
            <span className="w-[150px]">{t("protection.colStatus")}</span>
          </div>
          <ul>
            {table.crashes.map((c) => (
              <li key={c.id} className="flex items-start gap-3 border-b border-border/60 py-2">
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="text-[12.5px] font-medium">{c.name}</span>
                  <span className="text-[11.5px] leading-snug text-muted-foreground">{c.what}</span>
                  {c.writeup && (
                    <button
                      type="button"
                      onClick={() => void openUrl(c.writeup!)}
                      className="self-start text-[11.5px] text-primary hover:underline"
                    >
                      {t("protection.writeup")}
                    </button>
                  )}
                </div>
                <span className="w-[64px] shrink-0 text-right text-[12px] tabular-nums">{c.share}%</span>
                <span className={cn("w-[150px] shrink-0 text-[12px]", STATUS[c.status].tone)}>
                  {t(STATUS[c.status].label)}
                  {c.fixedIn && c.status !== "open" && c.status !== "outside" && (
                    <span className="block text-[11px] text-muted-foreground">
                      {t("protection.fixedIn", { v: c.fixedIn })}
                    </span>
                  )}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      <HostFrostServer />
    </div>
  );
}
