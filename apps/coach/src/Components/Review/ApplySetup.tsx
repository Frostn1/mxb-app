import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import {
  coachApplyPreview,
  coachApplySetup,
  coachUndoSetup,
  type ApplyFile,
  type ApplyPreview,
  type SetupDiff,
} from "@/api/coach";

/** One value of a change: what the bike's file says, else its position in the list. */
const shown = (value: string | null, at: number) => value ?? String(at);

function Diff({ d }: { d: SetupDiff }) {
  const t = useT();
  return (
    <li className="grid grid-cols-[1fr_auto] items-baseline gap-x-3">
      <span className="text-[12.5px] text-foreground">{t(`setupField.${d.field}` as TKey)}</span>
      <span className="whitespace-nowrap font-mono text-[12px]">
        <span className="text-muted-foreground">{shown(d.fromValue, d.from)}</span>
        <span className="px-1.5 text-faint">→</span>
        <span className="text-accent-foreground">{shown(d.toValue, d.to)}</span>
      </span>
    </li>
  );
}

/**
 * Writes the coach's changes into the rider's own setups, instead of a copy. Every setup they
 * have for this bike is listed across all their profile folders with the exact before and after
 * of each setting, they tick the ones they want, and each is backed up beside itself first.
 * Undo puts the backup back. Nothing is written while MX Bikes is running.
 */
export default function ApplySetup({ path, skills, onClose }: { path: string; skills: string[]; onClose: () => void }) {
  const t = useT();
  const [preview, setPreview] = useState<ApplyPreview | null>(null);
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = () =>
    coachApplyPreview(path, skills)
      .then((p) => {
        setPreview(p);
        // A pick that's gone, or has nothing left to change, is dropped.
        setPicked((v) => v.filter((f) => p.files.some((x) => x.path === f && x.changes.length > 0)));
      })
      .catch((e) => setError(String(e)));
  useEffect(() => {
    setPreview(null);
    setPicked([]);
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, skills]);

  const toggle = (f: ApplyFile) => setPicked((v) => (v.includes(f.path) ? v.filter((x) => x !== f.path) : [...v, f.path]));

  const report = (out: { ok: boolean; name: string; error: string | null }[], done: TKey) => {
    const good = out.filter((r) => r.ok).length;
    if (good > 0) toast.success(t(done, { n: good }));
    for (const r of out.filter((x) => !x.ok)) toast.error(t("apply.failed", { name: r.name, error: r.error ?? "" }));
  };

  const run = async (job: () => Promise<{ ok: boolean; name: string; error: string | null }[]>, done: TKey) => {
    setBusy(true);
    try {
      report(await job(), done);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
      await load();
    }
  };

  const apply = () => run(() => coachApplySetup(path, skills, picked), "apply.applied");
  const undo = (f: ApplyFile) => run(() => coachUndoSetup(path, [f.path]), "apply.undone");

  const gameOpen = preview?.gameOpen ?? false;
  const where = (f: ApplyFile) => [f.profile, f.place === "common" ? t("apply.common") : f.place].filter(Boolean).join(" · ");

  return (
    <div className="space-y-3 border-b border-border pb-4">
      <div className="flex items-baseline justify-between gap-3">
        <div className="text-[12.5px] font-semibold text-foreground">{t("apply.title")}</div>
        <button type="button" className="text-[12px] text-muted-foreground hover:text-foreground" onClick={onClose}>
          {t("apply.close")}
        </button>
      </div>
      <p className="text-[12px] text-muted-foreground">{t("apply.body")}</p>

      {gameOpen && <p className="text-[12px] text-warning">{t("apply.gameOpen")}</p>}
      {error && <p className="text-[12px] text-destructive">{error}</p>}
      {!preview && !error && <p className="text-[12px] text-muted-foreground">{t("apply.loading")}</p>}
      {preview && preview.files.length === 0 && <p className="text-[12px] text-muted-foreground">{t("apply.none")}</p>}

      <ul className="divide-y divide-border border-y border-border">
        {preview?.files.map((f) => {
          const can = f.changes.length > 0 && !f.problem;
          return (
            <li key={f.path} className="py-2.5">
              <div className="flex items-start gap-3">
                <input
                  type="checkbox"
                  className="mt-1 size-3.5"
                  checked={picked.includes(f.path)}
                  disabled={!can || busy}
                  onChange={() => toggle(f)}
                  aria-label={f.name}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-baseline gap-x-2">
                    <span className="truncate text-[13px] font-medium text-foreground">{f.name}</span>
                    <span className="text-[11.5px] text-muted-foreground">{where(f)}</span>
                  </div>
                  {f.problem ? (
                    <p className="mt-1 text-[12px] text-muted-foreground">{f.problem}</p>
                  ) : f.changes.length === 0 ? (
                    <p className="mt-1 text-[12px] text-muted-foreground">{t("apply.nothing")}</p>
                  ) : (
                    <ul className={cn("mt-1.5 space-y-1 border-l border-border pl-3")}>
                      {f.changes.map((d) => (
                        <Diff key={d.field} d={d} />
                      ))}
                    </ul>
                  )}
                </div>
                {f.backups > 0 && (
                  <Button size="sm" variant="outline" disabled={busy || gameOpen} onClick={() => undo(f)}>
                    {t("apply.undo")}
                  </Button>
                )}
              </div>
            </li>
          );
        })}
      </ul>

      <div className="flex justify-end">
        <Button size="sm" onClick={apply} disabled={busy || gameOpen || picked.length === 0}>
          {picked.length === 0 ? t("apply.goNone") : t("apply.go", { n: picked.length })}
        </Button>
      </div>
    </div>
  );
}
