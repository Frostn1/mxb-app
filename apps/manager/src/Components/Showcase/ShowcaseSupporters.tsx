import { ChevronRight, Coffee } from "lucide-react";
import { useI18n } from "../../i18n/context";
import { groupByTier } from "../Settings/supporters";
import { useSupporters } from "../Settings/useSupporters";

/** Enough to feel like a crowd, few enough to stay on two lines at 520px. */
const SHOWN = 5;

interface ShowcaseSupportersProps {
  /** Close the modal and land on Settings → Supporters. */
  onOpen: () => void;
}

/**
 * Credits the people paying for the app, inside the "what's new" modal.
 *
 * Deliberately not part of any release's entry in `releases.ts`: this is shown with
 * *every* release, so it can't be something the next version has to remember to add.
 * The names come from the same live manifest Settings reads, so a supporter who joined
 * after the build shipped is still thanked by it.
 *
 * It credits rather than asks — the block routes to the thank-you page in Settings, and
 * the "buy a coffee" button lives there. An update notice is a bad place to hold out a
 * hat, and a nag is what gets this dismissed unread.
 */
export default function ShowcaseSupporters({ onOpen }: ShowcaseSupportersProps) {
  const { t } = useI18n();
  const { manifest } = useSupporters();

  // Best tier first, manifest order within it — `groupByTier` already orders both, so
  // flattening it keeps Settings and this strip naming the same people in the same order.
  const people = groupByTier(manifest).flatMap((g) => g.people);
  const count = people.length;
  // A fetch that legitimately returns nobody shouldn't leave "made possible by 0
  // supporters" sitting under the release notes.
  if (count === 0) return null;

  const shown = people.slice(0, SHOWN);
  const rest = count - shown.length;

  return (
    <button
      onClick={onOpen}
      className="group flex cursor-default flex-col gap-2 rounded-xl border border-amber-400/20 bg-amber-400/[0.06] p-3 text-left transition-colors hover:border-amber-400/35 hover:bg-amber-400/[0.09]"
    >
      <div className="flex items-center gap-2">
        <Coffee className="size-3.5 flex-none text-amber-400/90" />
        <span className="text-[11.5px] font-semibold text-foreground/85">
          {t("showcase.supporters.title", { count })}
        </span>
        <ChevronRight className="ml-auto size-3.5 flex-none text-muted-foreground transition-transform group-hover:translate-x-0.5" />
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        {shown.map((person) => (
          <span
            key={person.name}
            className="max-w-[190px] truncate rounded-full border border-amber-400/20 bg-background/50 px-2 py-0.5 text-[11.5px] text-foreground/80"
          >
            {person.name}
          </span>
        ))}
        {rest > 0 && (
          <span className="text-[11.5px] text-muted-foreground">
            {t("showcase.supporters.more", { count: rest })}
          </span>
        )}
      </div>
    </button>
  );
}
