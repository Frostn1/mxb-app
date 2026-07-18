import { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCw, Loader2, AlertTriangle, Save, Play } from "lucide-react";
import { toast } from "sonner";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from "../ui/select";
import {
  presetsListProfiles,
  presetsListBikes,
  presetsReadLoadout,
  presetsApply,
  presetsSave,
} from "../../api/mods";
import type { Loadout, PresetApplyOutcome } from "../../types";
import { ViewerPanel } from "../Viewer/ViewerPanel";
import { SlotField } from "../Presets/SlotField";
import {
  SLOTS,
  SLOT_GROUPS,
  EMPTY_LOADOUT,
  loadScans,
  slotOptions,
  isMissing,
  missingSlots,
  type Scans,
} from "../../lib/presets";

/** The rider studio shows only the slots that dress the player model — the bike
 * livery / model-swap / tyres group is left to the Presets builder. */
const RIDER_GROUPS = SLOT_GROUPS.filter((g) => g.id !== "bike");

/** Turn an apply outcome into a short "how it took effect" note (rider looks live
 * only when the game re-reads the profile, so mirror Presets' messaging). */
function applyNote(outcome: PresetApplyOutcome): string {
  if (outcome.live_refresh === "refreshed") return "refreshed live in-game";
  if (outcome.game_running) return "reselect your profile in-game to see it";
  return "loads next time the game launches";
}

/**
 * Rider render studio — pick a helmet, goggles, outfit and boots (plus their
 * paints) and see them composed on the real player model at once, then apply the
 * look to a profile/bike or save it as a preset. Reuses the preset builder's
 * installed-mod index, slot fields, rider preview, and apply/save commands; it
 * just scopes the UI to the rider and drops the bike slots.
 */
export default function RiderStudio() {
  const [profiles, setProfiles] = useState<string[]>([]);
  const [profile, setProfile] = useState<string>("");
  const [bikes, setBikes] = useState<string[]>([]);
  const [bike, setBike] = useState<string>("");
  const [scans, setScans] = useState<Scans | null>(null);
  const [loadout, setLoadout] = useState<Loadout>(EMPTY_LOADOUT);
  const [makeActive, setMakeActive] = useState(true);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [applying, setApplying] = useState(false);

  const setSlot = useCallback((key: keyof Loadout, value: string) => {
    setLoadout((prev) => ({ ...prev, [key]: value }));
  }, []);

  // Initial load: profiles + the installed-mod index that feeds the pickers.
  const load = useCallback(async () => {
    setError(null);
    try {
      const [profs, sc] = await Promise.all([presetsListProfiles(), loadScans()]);
      setProfiles(profs);
      setScans(sc);
      setProfile((p) => p || profs[0] || "");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // Refresh the bike list when the profile changes (rider looks are stored
  // per-bike in profile.ini, so applying still targets a bike).
  useEffect(() => {
    if (!profile) {
      setBikes([]);
      return;
    }
    let cancelled = false;
    presetsListBikes(profile)
      .then((bs) => {
        if (cancelled) return;
        setBikes(bs);
        setBike((b) => (bs.includes(b) ? b : bs[0] || ""));
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [profile]);

  // Start from the look already on the target bike so the studio reflects
  // reality — but keep only the rider slots (leave the bike slots stock here).
  const capture = useCallback(async () => {
    if (!profile || !bike) return;
    try {
      const lo = await presetsReadLoadout(profile, bike);
      setLoadout({
        ...EMPTY_LOADOUT,
        rider: lo.rider,
        helmet: lo.helmet,
        helmetPaint: lo.helmetPaint,
        gogglesPaint: lo.gogglesPaint,
        suitPaint: lo.suitPaint,
        suitFont: lo.suitFont,
        boots: lo.boots,
        bootsPaint: lo.bootsPaint,
        glovesPaint: lo.glovesPaint,
        protection: lo.protection,
        protectionPaint: lo.protectionPaint,
        ridingStyle: lo.ridingStyle,
        raceNumber: lo.raceNumber,
      });
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    }
  }, [profile, bike]);

  useEffect(() => {
    void capture();
  }, [capture]);

  const grouped = useMemo(
    () => RIDER_GROUPS.map((g) => ({ ...g, slots: SLOTS.filter((s) => s.group === g.id) })),
    [],
  );

  const builderMissing = useMemo(
    () => (scans ? missingSlots(bike, loadout, scans).length : 0),
    [scans, bike, loadout],
  );

  const onApply = useCallback(async () => {
    if (!profile || !bike) {
      toast.error("Pick a profile and bike to apply to.");
      return;
    }
    setApplying(true);
    try {
      const outcome = await presetsApply(profile, bike, loadout, makeActive);
      toast.success(`Applied to ${bike} — ${applyNote(outcome)}`);
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setApplying(false);
    }
  }, [profile, bike, loadout, makeActive]);

  const onSave = useCallback(async () => {
    const nm = name.trim();
    if (!nm) {
      toast.error("Give the look a name first.");
      return;
    }
    setSaving(true);
    try {
      await presetsSave({ name: nm, loadout });
      setName("");
      toast.success(`Saved “${nm}” to Presets.`);
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setSaving(false);
    }
  }, [name, loadout]);

  const noProfiles = profiles.length === 0 && !error;

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <h1 className="text-[21px] font-bold tracking-[-0.2px]">Rider</h1>
        <p className="hidden text-[12.5px] text-muted-foreground sm:block">
          Dress the player model — helmet, goggles, outfit and boots together.
        </p>
        <div className="ml-auto flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={() => void load()}>
            <RefreshCw className="size-3.5" />
            Refresh
          </Button>
        </div>
      </header>

      {error && (
        <div className="mx-7 mb-3 flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-[12.5px] text-destructive">
          <AlertTriangle className="size-4" />
          {error}
        </div>
      )}

      {noProfiles ? (
        <div className="flex flex-1 items-center justify-center px-7 text-center text-[13px] text-muted-foreground">
          No MX Bikes profiles found. Launch the game once so it creates a profile,
          then refresh.
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 gap-5 overflow-hidden px-7 pb-6">
          {/* Picker column */}
          <section className="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
            {/* Target row */}
            <div className="flex flex-wrap items-end gap-3 rounded-xl border border-white/[0.07] bg-card/40 p-3.5">
              <label className="flex min-w-[140px] flex-col gap-1">
                <span className="text-[11px] font-medium text-muted-foreground">Profile</span>
                <Select value={profile} onValueChange={setProfile}>
                  <SelectTrigger>
                    <SelectValue placeholder="Profile" />
                  </SelectTrigger>
                  <SelectContent>
                    {profiles.map((p) => (
                      <SelectItem key={p} value={p}>
                        {p}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
              <label className="flex min-w-[180px] flex-1 flex-col gap-1">
                <span className="text-[11px] font-medium text-muted-foreground">Bike</span>
                <Select value={bike} onValueChange={setBike}>
                  <SelectTrigger>
                    <SelectValue placeholder="Bike" />
                  </SelectTrigger>
                  <SelectContent>
                    {bikes.map((b) => (
                      <SelectItem key={b} value={b}>
                        {b}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
              <Button variant="outline" size="sm" onClick={() => void capture()}>
                <RefreshCw className="size-3.5" />
                Capture current
              </Button>
            </div>

            {/* Rider slot groups */}
            {grouped.map((g) => (
              <div key={g.id} className="flex flex-col gap-2">
                <h2 className="text-[11px] font-semibold uppercase tracking-wide text-faint">
                  {g.label}
                </h2>
                <div className="grid grid-cols-1 gap-x-4 gap-y-2.5 sm:grid-cols-2">
                  {g.slots.map((slot) => (
                    <SlotField
                      key={slot.key}
                      slot={slot}
                      value={loadout[slot.key]}
                      options={scans ? slotOptions(slot, bike, loadout, scans) : []}
                      missing={scans ? isMissing(slot, bike, loadout, scans) : false}
                      onChange={(v) => setSlot(slot.key, v)}
                    />
                  ))}
                </div>
              </div>
            ))}

            {/* Race number + save/apply row */}
            <div className="flex flex-col gap-3 rounded-xl border border-white/[0.07] bg-card/40 p-3.5">
              <div className="flex flex-wrap items-center gap-3">
                <label className="flex items-center gap-2">
                  <span className="text-[11px] font-medium text-muted-foreground">Race #</span>
                  <Input
                    value={loadout.raceNumber}
                    onChange={(e) => setSlot("raceNumber", e.target.value)}
                    className="h-8 w-16"
                    placeholder="—"
                  />
                </label>
                <label className="ml-auto flex items-center gap-2 text-[12px] text-muted-foreground">
                  <Switch checked={makeActive} onCheckedChange={setMakeActive} />
                  Make this the active bike
                </label>
              </div>
              {builderMissing > 0 && (
                <p className="flex items-center gap-1.5 text-[11.5px] text-amber-500">
                  <AlertTriangle className="size-3.5" />
                  {builderMissing} slot{builderMissing > 1 ? "s" : ""} reference a mod
                  that isn't installed — it'll show as stock in-game.
                </p>
              )}
              <div className="flex flex-wrap items-center gap-2">
                <Input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="Save as preset…"
                  className="h-9 max-w-[220px]"
                  onKeyDown={(e) => e.key === "Enter" && void onSave()}
                />
                <Button size="sm" onClick={() => void onSave()} disabled={saving}>
                  {saving ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
                  Save preset
                </Button>
                <Button variant="outline" size="sm" onClick={() => void onApply()} disabled={applying}>
                  {applying ? <Loader2 className="size-3.5 animate-spin" /> : <Play className="size-3.5" />}
                  Apply now
                </Button>
              </div>
            </div>
          </section>

          {/* Live rider render */}
          <ViewerPanel loadout={loadout} riderOnly className="w-[420px] flex-none" />
        </div>
      )}
    </div>
  );
}
