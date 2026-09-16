import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import { coachSetCuePos, type Hud } from "@/api/coach";

/** The nine places the live cue can sit, as fractions of the screen. The middle of the grid
 *  is where the recorder has always put it, so a rider who never touches this sees no change. */
const X = [0.2, 0.5, 0.8];
const Y = [0.12, 0.31, 0.86];
const X_KEYS: TKey[] = ["hud.cuePos.left", "hud.cuePos.centre", "hud.cuePos.right"];
const Y_KEYS: TKey[] = ["hud.cuePos.top", "hud.cuePos.middle", "hud.cuePos.bottom"];

/** The cell nearest where the cue actually is, so a position set by hand in the file still
 *  lights up the square it is closest to rather than none at all. */
const nearest = (v: number, of: number[]) =>
  of.reduce((best, _, i) => (Math.abs(of[i] - v) < Math.abs(of[best] - v) ? i : best), 0);

/** Where the live cue shows on screen. Writes `cue_x` and `cue_y` to `hud.ini`, which the
 *  recorder from FrostMod 0.24 reads; an older one ignores them and draws where it always did. */
export default function CuePosition({ hud, onChange }: { hud: Hud; onChange: (hud: Hud) => void }) {
  const t = useT();
  const [cx, cy] = [nearest(hud.cuePos[0], X), nearest(hud.cuePos[1], Y)];
  const pick = (ix: number, iy: number) =>
    coachSetCuePos(X[ix], Y[iy])
      .then((h) => {
        onChange(h);
        toast.success(t("hud.cuePosSaved"));
      })
      .catch((e) => toast.error(String(e)));

  return (
    <div className="border-t border-border pt-3">
      <div className="text-[13px] font-semibold">{t("hud.cuePos")}</div>
      <p className="mt-0.5 text-[12px] text-muted-foreground">{t("hud.cuePosBody")}</p>
      <div className="mt-2.5 grid w-[104px] grid-cols-3 gap-1">
        {Y.map((_, iy) =>
          X.map((_, ix) => {
            const on = ix === cx && iy === cy;
            return (
              <button
                key={`${ix}-${iy}`}
                type="button"
                aria-pressed={on}
                aria-label={t("hud.cuePos.pick", { y: t(Y_KEYS[iy]), x: t(X_KEYS[ix]) })}
                onClick={() => void pick(ix, iy)}
                className={cn(
                  "h-8 border",
                  on ? "border-accent-foreground bg-accent-foreground/15" : "border-border hover:border-foreground/40",
                )}
              />
            );
          }),
        )}
      </div>
    </div>
  );
}
