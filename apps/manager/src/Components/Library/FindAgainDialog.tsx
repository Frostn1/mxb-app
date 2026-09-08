import { useEffect, useState } from "react";
import { Loader2, ExternalLink, PackageSearch } from "lucide-react";
import { searchMods } from "../../api/mods";
import { shopCatalogSearch } from "../../api/shop";
import type { LedgerRow, ShopPrice } from "../../types";
import { useT } from "../../i18n/context";
import { displayName } from "../../lib/mods";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "../ui/dialog";

/** One hit, flattened to the little every source can supply. */
interface Hit {
  title: string;
  /** Set for catalog mods — enough to open the mod's page in Browse. */
  slug?: string;
  thumb?: string | null;
  detail?: string;
}

/**
 * Everywhere a lost mod might be found again.
 *
 * The whole point of the list being a list: adding a source later means one entry here, not
 * another button on the card and another branch in the dialog. Each one is asked the same
 * question — *have you got something by this name* — and answers in the same shape.
 */
interface Source {
  id: string;
  label: string;
  find: (name: string) => Promise<Hit[]>;
}

/** The shop's price as one short string, or nothing when it doesn't quote one. */
function priceLabel(price: ShopPrice): string | undefined {
  const shown = price.onSale ? price.sale : price.base;
  return shown == null ? undefined : `${shown}`;
}

const SOURCES: Source[] = [
  {
    id: "mxbmods",
    label: "mxb-mods.com",
    // Category 0 means "don't narrow": the ledger knows the mod's category, but a track
    // filed under the wrong one on the site would then never be found.
    find: async (name) =>
      (await searchMods(name, 0, 1, "newest")).slice(0, 6).map((m) => ({
        title: m.title,
        slug: m.slug,
        thumb: m.image,
      })),
  },
  {
    id: "shop",
    label: "mxbikes-shop.com",
    find: async (name) =>
      (await shopCatalogSearch(name, null, 1, "nameAsc", false)).items
        .slice(0, 6)
        .map((p) => ({
          title: p.title,
          thumb: p.image,
          detail: priceLabel(p.price),
        })),
  },
];

type SourceState =
  | { status: "loading" }
  | { status: "done"; hits: Hit[] }
  | { status: "failed"; error: string };

/**
 * Search every known source for a mod that is no longer installed.
 *
 * The name is all a deleted mod leaves behind, so this is what turns the ledger from a list
 * of regrets into a way back. Sources are searched together and reported apart, because one
 * of them being down or signed-out shouldn't hide the others' answers.
 */
export default function FindAgainDialog({
  row,
  onOpenMod,
  onClose,
}: {
  row: LedgerRow;
  onOpenMod?: (slug: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const name = row.title?.trim() || displayName(row.name);
  const [states, setStates] = useState<Record<string, SourceState>>({});

  useEffect(() => {
    let alive = true;
    setStates(Object.fromEntries(SOURCES.map((s) => [s.id, { status: "loading" as const }])));
    for (const source of SOURCES) {
      source
        .find(name)
        .then((hits) => {
          if (alive) setStates((p) => ({ ...p, [source.id]: { status: "done", hits } }));
        })
        .catch((e) => {
          if (alive)
            setStates((p) => ({ ...p, [source.id]: { status: "failed", error: String(e) } }));
        });
    }
    return () => {
      alive = false;
    };
  }, [name]);

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("library.findAgain")}</DialogTitle>
          <DialogDescription>{t("library.findAgainFor", { name })}</DialogDescription>
        </DialogHeader>

        <div className="flex max-h-[52vh] flex-col gap-4 overflow-y-auto">
          {SOURCES.map((source) => {
            const state = states[source.id] ?? { status: "loading" as const };
            return (
              <section key={source.id} className="flex flex-col gap-2">
                <div className="flex items-center gap-2">
                  <span className="text-[12px] font-bold uppercase tracking-[1.1px] text-faint">
                    {source.label}
                  </span>
                  {state.status === "loading" && (
                    <Loader2 className="size-3 animate-spin text-faint" />
                  )}
                  {state.status === "done" && (
                    <span className="text-[11px] text-faint">{state.hits.length}</span>
                  )}
                </div>

                {state.status === "failed" && (
                  <p className="text-[12px] text-faint">{t("library.findAgainFailed")}</p>
                )}
                {state.status === "done" && state.hits.length === 0 && (
                  <p className="text-[12px] text-faint">{t("library.findAgainNone")}</p>
                )}
                {state.status === "done" &&
                  state.hits.map((hit, i) => {
                    const clickable = !!hit.slug && !!onOpenMod;
                    return (
                      <button
                        key={`${source.id}-${i}`}
                        disabled={!clickable}
                        onClick={() => {
                          if (hit.slug && onOpenMod) {
                            onOpenMod(hit.slug);
                            onClose();
                          }
                        }}
                        className="flex items-center gap-3 rounded-lg border border-white/[0.07] bg-card p-2 text-left transition-colors enabled:hover:border-white/20 disabled:cursor-default"
                      >
                        <div className="grid h-9 w-[56px] flex-none place-items-center overflow-hidden rounded bg-black/30 text-foreground/25">
                          {hit.thumb ? (
                            <img src={hit.thumb} alt="" className="h-full w-full object-cover" />
                          ) : (
                            <PackageSearch className="size-4" strokeWidth={1.5} />
                          )}
                        </div>
                        <span className="min-w-0 flex-1 truncate text-[12.5px]">{hit.title}</span>
                        {hit.detail && (
                          <span className="flex-none text-[11px] text-faint">{hit.detail}</span>
                        )}
                        {clickable && <ExternalLink className="size-3.5 flex-none text-faint" />}
                      </button>
                    );
                  })}
              </section>
            );
          })}
        </div>
      </DialogContent>
    </Dialog>
  );
}
