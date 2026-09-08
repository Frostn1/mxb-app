import { useState } from "react";
import { Download, Info } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import CachedImg from "@frost/shared/Components/ui/cached-img";
import RatingStars from "./RatingStars";
import type { ModRating, ModSummary } from "@frost/shared/types";
import { formatDateShort } from "@frost/shared/lib/mods";
import { useT } from "@frost/shared/i18n/context";

interface FeaturedModProps {
  mod: ModSummary;
  rating?: ModRating;
  installed: boolean;
  onOpen: () => void;
  onInstall: () => void;
}

/**
 * The banner at the top of Browse.
 *
 * It is the newest mod in whatever is being browsed, not an editorial pick — nothing in
 * the catalog marks a mod as featured, and inventing a server-side notion of one to fill a
 * banner would be the tail wagging the dog. `Browse` only renders it on the default sort
 * with no search or category filter, where "newest" is a claim the page is already making.
 *
 * It draws nothing without artwork: a hero is the artwork, and a 300px band holding a
 * placeholder icon is worse than no band at all.
 */
export default function FeaturedMod({
  mod,
  rating,
  installed,
  onOpen,
  onInstall,
}: FeaturedModProps) {
  const t = useT();
  const [broken, setBroken] = useState(false);
  if (!mod.image || broken) return null;

  return (
    <div className="relative mb-5 h-[276px] overflow-hidden bg-card">
      <CachedImg
        src={mod.image}
        width={1280}
        alt={mod.title}
        onUnavailable={() => setBroken(true)}
        className="absolute inset-0 size-full object-cover"
      />
      {/* Two scrims, not one: the horizontal keeps the copy legible over a busy photo, the
          vertical settles the band into the grid below instead of ending on a hard edge. */}
      <div className="absolute inset-0 bg-gradient-to-r from-[rgba(6,6,7,0.94)] via-[rgba(6,6,7,0.66)] to-transparent" />
      <div className="absolute inset-x-0 bottom-0 h-24 bg-gradient-to-t from-background to-transparent" />

      <div className="absolute inset-y-0 left-0 flex max-w-[620px] flex-col justify-end p-8">
        <div className="mb-3 flex items-center gap-2.5">
          <span className="u-skew bg-primary px-2 py-[3px]">
            <span className="u-unskew block font-cond text-[10px] font-bold uppercase tracking-[0.22em] text-primary-foreground">
              {t("browse.featured")}
            </span>
          </span>
          {rating && rating.count > 0 && <RatingStars rating={rating} />}
        </div>

        <h2 className="font-cond text-[46px] font-bold uppercase leading-[0.92] tracking-[0.005em] text-white">
          {mod.title}
        </h2>

        <div className="mt-3 flex items-center gap-2.5 text-[12.5px] text-white/60">
          <span className="tabular-figures">{formatDateShort(mod.date)}</span>
          {mod.author && (
            <>
              <span className="text-white/25">/</span>
              <span className="truncate">{mod.author}</span>
            </>
          )}
        </div>

        <div className="mt-5 flex items-center gap-2.5">
          <Button onClick={onInstall}>
            <Download className="size-4" />
            {installed ? t("browse.quickReinstall") : t("browse.quickInstall")}
          </Button>
          <Button variant="outline" onClick={onOpen}>
            <Info className="size-4" />
            {t("browse.openDetails")}
          </Button>
        </div>
      </div>
    </div>
  );
}
