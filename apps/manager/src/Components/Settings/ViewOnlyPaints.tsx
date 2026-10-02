import { useCallback, useEffect, useState } from "react";
import { Loader2, Lock, Eye, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Switch } from "@frost/shared/Components/ui/switch";
import {
  paintPolicies,
  setPaintPolicy,
  type OwnPaint,
  type OwnPaints,
  type PaintTeamEntry,
} from "@frost/shared/api/mods";
import { useT } from "@/i18n";

/** The GUID shape a server reads off a connection: `FF` and 16 hex digits. */
const GUID = /^FF[0-9A-F]{16}$/i;

function entryFor(text: string): PaintTeamEntry | null {
  const v = text.trim();
  if (!v) return null;
  return GUID.test(v) ? { kind: "guid", id: v.toUpperCase() } : { kind: "account", id: v };
}

const same = (a: PaintTeamEntry, b: PaintTeamEntry) =>
  a.kind === b.kind && a.id.toLowerCase() === b.id.toLowerCase();

/** One paint's row: two switches, a team list when locked, and Save. */
const PaintRow = ({
  paint,
  canLock,
  onSaved,
}: {
  paint: OwnPaint;
  canLock: boolean;
  onSaved: (p: OwnPaint) => void;
}) => {
  const t = useT();
  const [viewOnly, setViewOnly] = useState(paint.viewOnly);
  const [locked, setLocked] = useState(paint.locked);
  const [team, setTeam] = useState<PaintTeamEntry[]>(paint.team);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);

  const dirty =
    viewOnly !== paint.viewOnly ||
    locked !== paint.locked ||
    team.length !== paint.team.length ||
    team.some((e, i) => !same(e, paint.team[i]!));

  const add = () => {
    const e = entryFor(draft);
    if (!e || team.some((x) => same(x, e))) return;
    setTeam([...team, e]);
    setDraft("");
  };

  const save = async () => {
    setSaving(true);
    try {
      await setPaintPolicy(paint.sha256, viewOnly, locked, team);
      onSaved({ ...paint, viewOnly, locked, team });
      toast.success(t("viewOnly.saved"));
    } catch (e) {
      toast.error(t("viewOnly.saveFailed", { error: String(e) }));
    } finally {
      setSaving(false);
    }
  };

  const lockBlocked = !paint.lockable || (!canLock && !paint.locked);

  return (
    <div className="py-2.5">
      <div className="flex items-baseline gap-2">
        <span className="truncate text-[12.5px] text-foreground/85">{paint.fileName}</span>
        <span className="truncate text-[11px] text-muted-foreground">{paint.bikeId}</span>
        {!paint.worn && (
          <span className="text-[11px] text-muted-foreground">· {t("viewOnly.notWorn")}</span>
        )}
      </div>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-5 gap-y-1.5">
        <label className="flex items-center gap-2 text-[12px]" title={t("viewOnly.viewOnlyHint")}>
          <Switch checked={viewOnly} onCheckedChange={setViewOnly} />
          <Eye className="size-3.5 text-muted-foreground" />
          {t("viewOnly.viewOnly")}
        </label>
        <label
          className="flex items-center gap-2 text-[12px]"
          title={
            !paint.lockable
              ? t("viewOnly.notLockable")
              : !canLock
                ? t("viewOnly.needsSteam")
                : t("viewOnly.lockedHint")
          }
        >
          {/* Turning a lock off is always allowed, even when it could not be turned on now. */}
          <Switch checked={locked} onCheckedChange={setLocked} disabled={lockBlocked && !locked} />
          <Lock className="size-3.5 text-muted-foreground" />
          {t("viewOnly.locked")}
        </label>
        <Button size="sm" variant="secondary" disabled={!dirty || saving} onClick={() => void save()}>
          {saving ? <Loader2 className="size-3.5 animate-spin" /> : t("viewOnly.save")}
        </Button>
      </div>
      {locked && (
        <div className="mt-2 flex flex-col gap-1.5 pl-1">
          <div className="text-[11.5px] text-muted-foreground">{t("viewOnly.team")}</div>
          <div className="flex flex-wrap gap-1.5">
            {team.map((e) => (
              <span
                key={`${e.kind}:${e.id}`}
                className="flex items-center gap-1 rounded-md bg-foreground/[0.06] px-2 py-0.5 text-[11.5px]"
              >
                {e.id}
                <button
                  type="button"
                  aria-label={t("viewOnly.remove")}
                  className="text-muted-foreground hover:text-foreground"
                  onClick={() => setTeam(team.filter((x) => !same(x, e)))}
                >
                  <X className="size-3" />
                </button>
              </span>
            ))}
          </div>
          <div className="flex max-w-sm gap-1.5">
            <Input
              value={draft}
              placeholder={t("viewOnly.teamPlaceholder")}
              className="h-7 text-[12px]"
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") add();
              }}
            />
            <Button size="sm" variant="ghost" onClick={add} disabled={!entryFor(draft)}>
              {t("viewOnly.add")}
            </Button>
          </div>
        </div>
      )}
      {!canLock && paint.lockable && !paint.locked && (
        <div className="mt-1 text-[11px] text-muted-foreground">{t("viewOnly.needsSteam")}</div>
      )}
    </div>
  );
};

/**
 * The owner's control for view-only and locked paints.
 *
 * Says plainly what it is worth: view-only stops casual keeping, not a determined copy, and a
 * lock holds only on MXB servers that enforce it.
 */
export const ViewOnlyPaints = () => {
  const t = useT();
  const [state, setState] = useState<OwnPaints | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    paintPolicies()
      .then((s) => {
        setState(s);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);
  useEffect(load, [load]);

  const saved = (p: OwnPaint) =>
    setState((s) => (s ? { ...s, paints: s.paints.map((x) => (x.sha256 === p.sha256 ? p : x)) } : s));

  // Pulled while the control plane reports it unavailable: no controls, no dead toggles. Nothing
  // is shown while loading or on a failed load either.
  if (!state?.viewOnlyAvailable) return null;

  return (
    <div className="mt-4">
      <div className="text-[12.5px] font-semibold text-foreground/85">{t("viewOnly.title")}</div>
      <p className="mt-0.5 text-[11.5px] leading-relaxed text-muted-foreground">{t("viewOnly.desc")}</p>
      <p className="mt-1.5 text-[11px] leading-relaxed text-muted-foreground/90">{t("viewOnly.honest")}</p>
      {error ? (
        <p className="mt-2 text-[11.5px] text-warning">{t("viewOnly.loadFailed", { error })}</p>
      ) : !state ? (
        <Loader2 className="mt-2 size-4 animate-spin text-muted-foreground" />
      ) : state.paints.length === 0 ? (
        <p className="mt-2 text-[11.5px] text-muted-foreground">{t("viewOnly.empty")}</p>
      ) : (
        <div className="mt-1 divide-y divide-white/[0.05]">
          {state.paints.map((p) => (
            <PaintRow key={p.sha256} paint={p} canLock={state.canLock} onSaved={saved} />
          ))}
        </div>
      )}
    </div>
  );
};

export default ViewOnlyPaints;
