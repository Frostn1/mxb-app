import { ShieldCheck } from "lucide-react";
import { useT } from "@/i18n";
import { caughtThisWeek, fixedCount, namesOf, useProtection } from "../../lib/protection";

/**
 * The home strip: what FrostMod stopped this week, by name, and a way into the page.
 *
 * Always shown where FrostMod runs. Protection that only works in the background is protection
 * nobody knows they have, so a quiet week still says what it is guarding against.
 */
export default function ProtectionCard({ onOpen }: { onOpen: () => void }) {
  const t = useT();
  const { caught, table } = useProtection();
  const week = caughtThisWeek(caught);
  const names = namesOf(table, week);
  return (
    <button
      type="button"
      onClick={onOpen}
      className="flex w-full items-center gap-2.5 border-b border-border px-4 py-2 text-left hover:bg-card/60"
    >
      <ShieldCheck className="size-4 shrink-0 text-success" />
      <span className="min-w-0 flex-1 truncate text-[12.5px]">
        {week.length > 0 ? (
          <>
            <span className="font-semibold">{t("protection.cardWeek", { count: week.length })}</span>
            <span className="text-muted-foreground"> · {names.slice(0, 3).join(", ")}</span>
          </>
        ) : (
          <span className="text-muted-foreground">
            {t("protection.cardNone", { count: fixedCount(table) })}
          </span>
        )}
      </span>
      <span className="shrink-0 text-[12px] text-primary">{t("protection.cardOpen")}</span>
    </button>
  );
}
