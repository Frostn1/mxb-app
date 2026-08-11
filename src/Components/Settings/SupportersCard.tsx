import { Coffee, ExternalLink, RefreshCw } from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { useI18n } from "../../i18n/context";
import { getLocale } from "../../i18n/core";
import { Button } from "@/Components/ui/button";
import { cn } from "@/lib/utils";
import { SUPPORT_URL, groupByTier } from "./supporters";
import { useSupporters } from "./useSupporters";

/** "since Jul 2026". Month and year only — the day somebody pledged is noise, and the
 *  year on its own reads as though they might have stopped in January. */
function sinceLabel(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString(getLocale(), { month: "short", year: "numeric" });
}

/**
 * Credits the people paying for the app's development, and gives everyone else the one
 * button that joins them.
 *
 * Lives in Settings rather than behind a nag banner on purpose: it's a thank-you page
 * you can go and read, not something the app pushes at you mid-ride.
 */
export default function SupportersCard() {
  const { t } = useI18n();
  const { manifest, loading, stale, refresh } = useSupporters();
  const groups = groupByTier(manifest);
  const count = manifest.supporters.length;
  // One nameless group means no distinction is being drawn, and a "Supporters" heading
  // sitting directly under the "Supporters" section title — counting names the reader
  // can already see — labels that non-distinction. Headings come back the moment there
  // is more than one group, or the only group has a level of its own.
  const showHeadings = groups.length > 1 || groups[0]?.tier != null;
  // A URL the manifest carries wins, so a bad default can be corrected without a
  // release — see the TODO on `SUPPORT_URL`.
  const supportUrl = manifest.supportUrl || SUPPORT_URL;

  return (
    <div className="flex flex-col gap-3">
      <p className="text-[12px] leading-relaxed text-muted-foreground">
        {t("supporters.intro")}
      </p>

      <div className="flex items-center justify-between gap-2">
        <span className="text-[11.5px] text-muted-foreground">
          {count > 0
            ? t("supporters.count", { count })
            : loading
              ? t("supporters.loading")
              : ""}
        </span>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => void refresh()}
          disabled={loading}
        >
          <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
          {t("supporters.refresh")}
        </Button>
      </div>

      {count > 0 ? (
        <div className="flex flex-col gap-3">
          {groups.map((group) => (
            <div key={group.tier ?? "__untiered"} className="flex flex-col gap-1.5">
              {showHeadings && (
                <div className="flex items-baseline gap-1.5">
                  <span className="text-[11.5px] font-semibold text-foreground/80">
                    {group.tier ?? t("supporters.untiered")}
                  </span>
                  <span className="text-[11px] text-faint">{group.people.length}</span>
                </div>
              )}
              <div className="flex flex-wrap gap-1.5">
                {group.people.map((person) => (
                  <span
                    key={`${group.tier ?? ""}:${person.name}`}
                    className="flex items-center gap-1.5 rounded-full border border-input bg-foreground/[0.03] px-2.5 py-1 text-[12px] text-foreground/85"
                  >
                    <Coffee className="size-3 flex-none text-primary" />
                    {person.name}
                    {person.since && sinceLabel(person.since) && (
                      <span className="text-[10.5px] text-faint">
                        {t("supporters.since", { date: sinceLabel(person.since) })}
                      </span>
                    )}
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>
      ) : (
        !loading && (
          <div className="flex flex-col gap-1 rounded-lg border border-input bg-foreground/[0.03] p-3">
            <span className="text-[12px] font-semibold text-foreground/85">
              {t("supporters.empty")}
            </span>
            <span className="text-[11.5px] leading-relaxed text-muted-foreground">
              {t("supporters.emptyDesc")}
            </span>
          </div>
        )
      )}

      {/* Only after a fetch actually failed. Saying "this list may be out of date" on
          every render would put a warning under a list that is, in fact, current. */}
      {stale && (
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          {t("supporters.offline")}
        </p>
      )}

      <div className="flex gap-2 pt-0.5">
        <Button size="sm" onClick={() => void openUrl(supportUrl)}>
          <Coffee className="size-3.5" /> {t("supporters.become")}
          <ExternalLink className="size-3" />
        </Button>
      </div>

      <p className="text-[11px] leading-relaxed text-faint">
        {t("supporters.optOut")}
      </p>
    </div>
  );
}
