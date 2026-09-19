import type { ReactNode } from "react";
import { ShoppingBag } from "lucide-react";
import type { ModType } from "@frost/shared/api/mods";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";

/** One entry in the category half of the column. */
export interface TypeListCategory {
  id: number;
  label: string;
}

interface TypeListProps {
  modTypes: ModType[];
  modType: ModType;
  /** Per-type totals, for the sources that publish one. Absent is normal, not an error. */
  counts?: Map<string, number>;
  categories: TypeListCategory[];
  categoryId: number | null;
  onChangeType: (t: ModType) => void;
  onChangeCategory: (id: number) => void;
  /** The "My purchases" entry, when a store is the active source. */
  purchases?: { active: boolean; onSelect: () => void };
}

/**
 * What kind of mod you are after, down the left edge.
 *
 * Type was a tab row and category was a dropdown beside the sort, which put the two halves of
 * one decision in two different places — and folding three catalogues into one screen would
 * have stacked a third row on top. A column holds both, holds as many categories as a store
 * actually has, and leaves the bar above for the things that change what is already on screen:
 * search, sort, and where it comes from.
 */
export default function TypeList({
  modTypes,
  modType,
  counts,
  categories,
  categoryId,
  onChangeType,
  onChangeCategory,
  purchases,
}: TypeListProps) {
  const t = useT();
  const browsing = !purchases?.active;

  return (
    <div className="flex w-[216px] flex-none flex-col gap-4 overflow-y-auto border-r border-border px-3 pb-6 pt-1">
      <section className="flex flex-col gap-0.5">
        <Heading>{t("mods.type")}</Heading>
        {modTypes.map((mt) => (
          <Row
            key={mt.id}
            active={browsing && mt.id === modType.id}
            onSelect={() => onChangeType(mt)}
            trailing={counts?.get(mt.id)}
          >
            {t(mt.label)}
          </Row>
        ))}
      </section>

      {categories.length > 1 && (
        <section className="flex flex-col gap-0.5">
          <Heading>{t("mods.category")}</Heading>
          {categories.map((c) => (
            <Row
              key={c.id}
              active={browsing && categoryId === c.id}
              onSelect={() => onChangeCategory(c.id)}
            >
              {c.label}
            </Row>
          ))}
        </section>
      )}

      {purchases && (
        <section className="mt-auto flex flex-col gap-0.5 border-t border-border pt-3">
          <Row active={purchases.active} onSelect={purchases.onSelect} icon={ShoppingBag}>
            {t("shopTab.purchases")}
          </Row>
        </section>
      )}
    </div>
  );
}

function Heading({ children }: { children: ReactNode }) {
  return (
    <span className="px-2.5 pb-1 font-cond text-[10px] font-bold uppercase tracking-[0.16em] text-faint">
      {children}
    </span>
  );
}

function Row({
  active,
  onSelect,
  trailing,
  icon: Icon,
  children,
}: {
  active: boolean;
  onSelect: () => void;
  trailing?: number;
  icon?: typeof ShoppingBag;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onSelect}
      className={cn(
        "flex h-[30px] cursor-default items-center gap-2 rounded-lg px-2.5 text-left text-[12.5px] transition-colors",
        active
          ? "bg-primary/15 font-semibold text-primary"
          : "text-muted-foreground hover:bg-card hover:text-foreground",
      )}
    >
      {Icon && <Icon className="size-3.5 flex-none" />}
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {trailing !== undefined && trailing > 0 && (
        <span className={cn("tabular-figures text-[11px]", active ? "text-primary/70" : "text-faint")}>
          {trailing}
        </span>
      )}
    </button>
  );
}
