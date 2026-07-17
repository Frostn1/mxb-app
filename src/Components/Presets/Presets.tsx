import { useCallback, useEffect, useMemo, useState } from "react";
import {
  RefreshCw,
  Loader2,
  AlertTriangle,
  Save,
  Play,
  Share2,
  Download,
  Trash2,
  Copy,
  Check,
  Package,
  UploadCloud,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
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
  Dialog,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
} from "../ui/dialog";
import {
  presetsListProfiles,
  presetsListBikes,
  presetsReadLoadout,
  presetsApply,
  presetsList,
  presetsSave,
  presetsDelete,
  presetsExport,
  presetsDecode,
  presetsImport,
  presetBundleStats,
  presetBundleCreate,
  presetBundleImport,
  onPresetBundleProgress,
} from "../../api/mods";
import type {
  BundlePhase,
  BundlePlan,
  Loadout,
  Preset,
  PresetApplyOutcome,
} from "../../types";
import {
  SLOTS,
  SLOT_GROUPS,
  loadScans,
  slotOptions,
  isMissing,
  missingSlots,
  loadoutSummary,
  type Scans,
  type SlotDef,
} from "../../lib/presets";

const EMPTY: Loadout = {
  paint: "",
  bikeFont: "",
  rider: "",
  helmet: "",
  helmetPaint: "",
  gogglesPaint: "",
  suitPaint: "",
  suitFont: "",
  boots: "",
  bootsPaint: "",
  glovesPaint: "",
  protection: "",
  protectionPaint: "",
  ridingStyle: "",
  tyres: "",
  raceNumber: "",
  modelSwap: "",
};

/** Human-readable byte size for bundle previews. */
function humanSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

/** Human label for a bundle progress phase. */
function phaseLabel(phase: BundlePhase): string {
  switch (phase) {
    case "bundling":
      return "Packaging assets…";
    case "uploading":
      return "Uploading bundle…";
    case "downloading":
      return "Downloading bundle…";
    case "installing":
      return "Installing assets…";
    case "done":
      return "Done";
  }
}

/** Copy text to the clipboard, best-effort (webview clipboard, then execCommand). */
async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      document.body.removeChild(ta);
      return ok;
    } catch {
      return false;
    }
  }
}

/**
 * Turn an apply outcome into a user-facing "how it took effect" note.
 *
 * The *selected* look lives in the game's memory (read from `profile.ini` only
 * when a profile is selected), so a FrostMod content reload alone doesn't show
 * it. Priority: a live in-place refresh > "reselect your profile" while the game
 * runs > "loads next launch".
 */
function applyNote(outcome: PresetApplyOutcome): string {
  switch (outcome.live_refresh) {
    case "refreshed":
      return "refreshed live in-game.";
    case "failed":
      return "saved — instant refresh failed, so reselect your profile in-game to load it.";
    default:
      break;
  }
  if (outcome.game_running) {
    return "saved — reselect your profile in MX Bikes (Profile menu) to load the new look.";
  }
  return "saved — it loads next time the game opens.";
}

/**
 * Presets — build a full customization **loadout** (helmet, paints, boots, gloves,
 * suit, protection, tyres…) and apply it to a bike on command. MX Bikes stores the
 * selected look per-bike in `profile.ini`; a preset is a bike-agnostic bundle of
 * every slot value. You can capture the look a bike currently wears, edit any slot
 * from what you've installed, save it named, quick-apply it later, and share it as
 * a code others can import. Applying nudges a running FrostMod to reload.
 */
export default function Presets() {
  const [profiles, setProfiles] = useState<string[]>([]);
  const [profile, setProfile] = useState<string>("");
  const [bikes, setBikes] = useState<string[]>([]);
  const [bike, setBike] = useState<string>("");
  const [scans, setScans] = useState<Scans | null>(null);
  const [loadout, setLoadout] = useState<Loadout>(EMPTY);
  const [saved, setSaved] = useState<Preset[]>([]);
  const [makeActive, setMakeActive] = useState(true);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [applyingId, setApplyingId] = useState<string | null>(null);

  const [sharePreset, setSharePreset] = useState<Preset | null>(null);
  const [importOpen, setImportOpen] = useState(false);

  const setSlot = useCallback((key: keyof Loadout, value: string) => {
    setLoadout((prev) => ({ ...prev, [key]: value }));
  }, []);

  // Initial load: profiles, saved presets, installed-mod index.
  const load = useCallback(async () => {
    setError(null);
    try {
      const [profs, presets, sc] = await Promise.all([
        presetsListProfiles(),
        presetsList(),
        loadScans(),
      ]);
      setProfiles(profs);
      setSaved(presets);
      setScans(sc);
      setProfile((p) => p || profs[0] || "");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // When the profile changes, refresh its bike list.
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

  // Capture the current look whenever the target bike changes, so the builder
  // starts from what's actually on that bike (the "capture current" default).
  const capture = useCallback(async () => {
    if (!profile || !bike) return;
    try {
      setLoadout(await presetsReadLoadout(profile, bike));
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    }
  }, [profile, bike]);

  useEffect(() => {
    void capture();
  }, [capture]);

  const refreshSaved = useCallback(async () => {
    setSaved(await presetsList());
  }, []);

  const onSave = useCallback(async () => {
    const nm = name.trim();
    if (!nm) {
      toast.error("Give the preset a name first.");
      return;
    }
    setBusy(true);
    try {
      await presetsSave({ name: nm, loadout });
      await refreshSaved();
      setName("");
      toast.success(`Saved preset “${nm}”.`);
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [name, loadout, refreshSaved]);

  const applyLoadout = useCallback(
    async (lo: Loadout, id: string, label: string) => {
      if (!profile || !bike) {
        toast.error("Pick a profile and bike to apply to.");
        return;
      }
      setApplyingId(id);
      try {
        const outcome = await presetsApply(profile, bike, lo, makeActive);
        toast.success(`Applied “${label}” to ${bike} — ${applyNote(outcome)}`);
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setApplyingId(null);
      }
    },
    [profile, bike, makeActive],
  );

  const onShare = useCallback((preset: Preset) => {
    setSharePreset(preset);
  }, []);

  const onDelete = useCallback(
    async (preset: Preset) => {
      if (!window.confirm(`Delete preset “${preset.name}”?`)) return;
      try {
        await presetsDelete(preset.name);
        await refreshSaved();
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      }
    },
    [refreshSaved],
  );

  const grouped = useMemo(
    () => SLOT_GROUPS.map((g) => ({ ...g, slots: SLOTS.filter((s) => s.group === g.id) })),
    [],
  );

  const builderMissing = useMemo(
    () => (scans ? missingSlots(bike, loadout, scans).length : 0),
    [scans, bike, loadout],
  );

  const noProfiles = profiles.length === 0 && !error;

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <h1 className="text-[21px] font-bold tracking-[-0.2px]">Presets</h1>
        <p className="hidden text-[12.5px] text-muted-foreground sm:block">
          Save a full look and load it onto a bike on command.
        </p>
        <div className="ml-auto flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => setImportOpen(true)}>
            <Download className="size-3.5" />
            Import
          </Button>
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
          {/* Builder */}
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

            {/* Slot groups */}
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

            {/* Race number + save row */}
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
                  placeholder="Preset name…"
                  className="h-9 max-w-[220px]"
                  onKeyDown={(e) => e.key === "Enter" && void onSave()}
                />
                <Button size="sm" onClick={() => void onSave()} disabled={busy}>
                  {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
                  Save preset
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void applyLoadout(loadout, "__builder__", "current look")}
                  disabled={applyingId !== null}
                >
                  {applyingId === "__builder__" ? (
                    <Loader2 className="size-3.5 animate-spin" />
                  ) : (
                    <Play className="size-3.5" />
                  )}
                  Apply now
                </Button>
              </div>
            </div>
          </section>

          {/* Live 3D preview of the current loadout */}

          {/* Saved presets */}
          <aside className="flex w-[300px] flex-none flex-col gap-2 overflow-y-auto border-l border-white/[0.06] pl-5">
            <h2 className="text-[11px] font-semibold uppercase tracking-wide text-faint">
              Saved presets
            </h2>
            {saved.length === 0 ? (
              <p className="mt-2 text-[12px] text-muted-foreground">
                No presets yet. Build a look and save it, or import a shared code.
              </p>
            ) : (
              saved.map((p) => (
                <PresetCard
                  key={p.name}
                  preset={p}
                  applying={applyingId === p.name}
                  disabled={applyingId !== null}
                  onApply={() => void applyLoadout(p.loadout, p.name, p.name)}
                  onLoad={() => setLoadout(p.loadout)}
                  onShare={() => onShare(p)}
                  onDelete={() => void onDelete(p)}
                />
              ))
            )}
          </aside>
        </div>
      )}

      <ShareDialog preset={sharePreset} onClose={() => setSharePreset(null)} />
      <ImportDialog
        open={importOpen}
        scans={scans}
        bike={bike}
        onClose={() => setImportOpen(false)}
        onImported={async () => {
          await refreshSaved();
          setImportOpen(false);
        }}
      />
    </div>
  );
}

/** One editable slot: an input with a datalist of installed options (so unknown /
 * captured values and free-text fonts still work), plus a "missing mod" badge. */
function SlotField({
  slot,
  value,
  options,
  missing,
  onChange,
}: {
  slot: SlotDef;
  value: string;
  options: string[];
  missing: boolean;
  onChange: (v: string) => void;
}) {
  const listId = `slot-${slot.key}`;
  return (
    <label className="flex flex-col gap-1">
      <span className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
        {slot.label}
        {missing && (
          <span
            title="This mod isn't installed — shows as stock in-game"
            className="rounded bg-amber-500/15 px-1 text-[9.5px] font-semibold uppercase text-amber-500"
          >
            missing
          </span>
        )}
      </span>
      <Input
        list={options.length ? listId : undefined}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Stock"
        className={cn("h-8 text-[12.5px]", missing && "border-amber-500/40")}
      />
      {options.length > 0 && (
        <datalist id={listId}>
          {options.map((o) => (
            <option key={o} value={o} />
          ))}
        </datalist>
      )}
    </label>
  );
}

/** A saved preset row with load-into-builder, apply, share, and delete actions. */
function PresetCard({
  preset,
  applying,
  disabled,
  onApply,
  onLoad,
  onShare,
  onDelete,
}: {
  preset: Preset;
  applying: boolean;
  disabled: boolean;
  onApply: () => void;
  onLoad: () => void;
  onShare: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-white/[0.07] bg-card/50 p-3">
      <div className="flex items-start justify-between gap-2">
        <button
          onClick={onLoad}
          title="Load into the builder"
          className="min-w-0 flex-1 cursor-default text-left"
        >
          <div className="truncate text-[13px] font-semibold">{preset.name}</div>
          <div className="truncate text-[11px] text-muted-foreground">
            {loadoutSummary(preset.loadout)}
          </div>
        </button>
        <div className="flex flex-none items-center gap-0.5">
          <IconBtn title="Share" onClick={onShare}>
            <Share2 className="size-3.5" />
          </IconBtn>
          <IconBtn title="Delete" onClick={onDelete}>
            <Trash2 className="size-3.5" />
          </IconBtn>
        </div>
      </div>
      <Button size="sm" className="h-7 w-full" onClick={onApply} disabled={disabled}>
        {applying ? <Loader2 className="size-3.5 animate-spin" /> : <Play className="size-3.5" />}
        Apply
      </Button>
    </div>
  );
}

function IconBtn({
  title,
  onClick,
  children,
}: {
  title: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      className="cursor-default rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
    >
      {children}
    </button>
  );
}

/**
 * Share a preset two ways: the instant **config code** (recipient needs the mods),
 * or a **full bundle** — package every asset the look references, upload it, and
 * hand back a code with the download link baked in so a recipient who owns nothing
 * still gets the complete look.
 */
function ShareDialog({ preset, onClose }: { preset: Preset | null; onClose: () => void }) {
  const [copied, setCopied] = useState(false);
  const [configCode, setConfigCode] = useState<string | null>(null);
  const [fullCode, setFullCode] = useState<string | null>(null);
  const [plan, setPlan] = useState<BundlePlan | null>(null);
  const [creating, setCreating] = useState(false);
  const [phase, setPhase] = useState<BundlePhase | null>(null);

  // On open, fetch the plain config code and preview what a full bundle carries.
  useEffect(() => {
    if (!preset) return;
    setCopied(false);
    setConfigCode(null);
    setFullCode(null);
    setPlan(null);
    setPhase(null);
    let cancelled = false;
    presetsExport(preset.name)
      .then((c) => !cancelled && setConfigCode(c))
      .catch((e) => !cancelled && toast.error(String(e).replace(/^Error:\s*/, "")));
    presetBundleStats(preset.loadout)
      .then((p) => !cancelled && setPlan(p))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [preset]);

  const isFull = fullCode !== null;
  const code = fullCode ?? configCode;

  const createBundle = useCallback(async () => {
    if (!preset) return;
    setCreating(true);
    setPhase("bundling");
    const unlisten = await onPresetBundleProgress((p) => setPhase(p.phase));
    try {
      const c = await presetBundleCreate(preset.name);
      setFullCode(c);
      setCopied(false);
      toast.success("Full bundle uploaded — the code now includes the assets.");
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      unlisten();
      setCreating(false);
      setPhase(null);
    }
  }, [preset]);

  return (
    <Dialog open={!!preset} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Share “{preset?.name}”</DialogTitle>
          <DialogDescription>
            {isFull
              ? "This code includes a downloadable asset bundle — the recipient picks Full import and gets everything, even with no mods installed."
              : "Send this code to anyone. They import it under Presets → Import. They'll need the same mods installed for every part to show."}
          </DialogDescription>
        </DialogHeader>

        <textarea
          readOnly
          value={code ?? ""}
          onFocus={(e) => e.currentTarget.select()}
          placeholder="Generating code…"
          className="h-24 w-full resize-none rounded-lg border border-input bg-transparent p-2.5 font-mono text-[11px] leading-snug"
        />

        {/* Full-bundle section */}
        {!isFull && (
          <div className="rounded-lg border border-white/[0.07] bg-card/40 p-3 text-[12px]">
            <div className="flex items-center gap-1.5 font-semibold">
              <Package className="size-3.5" />
              Full bundle
            </div>
            {plan && (
              <p className="mt-1 text-muted-foreground">
                {plan.assets.length === 0
                  ? "No installed assets to bundle — this look is all stock/fonts."
                  : `Packages ${plan.assets.length} asset${plan.assets.length > 1 ? "s" : ""} (~${humanSize(plan.totalSize)}) so a recipient needs nothing installed.`}
                {plan.unresolved.length > 0 && plan.assets.length > 0 && (
                  <>
                    {" "}
                    Excludes: {plan.unresolved.map((u) => u.value).join(", ")}.
                  </>
                )}
              </p>
            )}
            <p className="mt-1.5 text-[11px] text-faint">
              Uploads to a public, temporary link — it redistributes mod files
              made by others, so share responsibly.
            </p>
            <Button
              variant="outline"
              size="sm"
              className="mt-2"
              disabled={creating || !plan || plan.assets.length === 0}
              onClick={() => void createBundle()}
            >
              {creating ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <UploadCloud className="size-3.5" />
              )}
              {creating ? (phase ? phaseLabel(phase) : "Working…") : "Create full bundle"}
            </Button>
          </div>
        )}

        <DialogFooter>
          <Button
            disabled={!code}
            onClick={async () => {
              if (code && (await copyText(code))) {
                setCopied(true);
                toast.success(isFull ? "Copied full-bundle code." : "Copied share code.");
              } else {
                toast.error("Couldn't copy — select the code and copy manually.");
              }
            }}
          >
            {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
            {copied ? "Copied" : isFull ? "Copy full code" : "Copy code"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Paste a share code, preview it (name + missing mods), then import. */
function ImportDialog({
  open,
  scans,
  bike,
  onClose,
  onImported,
}: {
  open: boolean;
  scans: Scans | null;
  bike: string;
  onClose: () => void;
  onImported: () => void;
}) {
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<Preset | null>(null);
  const [previewErr, setPreviewErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<BundlePhase | null>(null);

  useEffect(() => {
    if (!open) {
      setText("");
      setPreview(null);
      setPreviewErr(null);
      setPhase(null);
    }
  }, [open]);

  // Decode as the user pastes/edits, to preview the preset and its missing mods.
  useEffect(() => {
    const t = text.trim();
    if (!t) {
      setPreview(null);
      setPreviewErr(null);
      return;
    }
    let cancelled = false;
    presetsDecode(t)
      .then((p) => {
        if (cancelled) return;
        setPreview(p);
        setPreviewErr(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setPreview(null);
        setPreviewErr(String(e).replace(/^Error:\s*/, ""));
      });
    return () => {
      cancelled = true;
    };
  }, [text]);

  const missing = useMemo(
    () => (preview && scans ? missingSlots(bike, preview.loadout, scans) : []),
    [preview, scans, bike],
  );

  const onImport = useCallback(async () => {
    if (!preview) return;
    setBusy(true);
    try {
      await presetsImport(text.trim());
      toast.success(`Imported preset “${preview.name}”.`);
      onImported();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [preview, text, onImported]);

  const onFullImport = useCallback(async () => {
    if (!preview) return;
    setBusy(true);
    setPhase("downloading");
    const unlisten = await onPresetBundleProgress((p) => setPhase(p.phase));
    try {
      await presetBundleImport(text.trim());
      toast.success(`Imported “${preview.name}” with all assets installed.`);
      onImported();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      unlisten();
      setBusy(false);
      setPhase(null);
    }
  }, [preview, text, onImported]);

  const hasBundle = !!preview?.bundle;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Import preset</DialogTitle>
          <DialogDescription>Paste a share code someone sent you.</DialogDescription>
        </DialogHeader>
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="MXBP1-…"
          className="h-24 w-full resize-none rounded-lg border border-input bg-transparent p-2.5 font-mono text-[11px] leading-snug placeholder:text-faint focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        />
        {previewErr && text.trim() && (
          <p className="flex items-center gap-1.5 text-[12px] text-destructive">
            <AlertTriangle className="size-3.5" />
            {previewErr}
          </p>
        )}
        {preview && (
          <div className="rounded-lg border border-white/[0.07] bg-card/40 p-2.5 text-[12px]">
            <div className="font-semibold">{preview.name}</div>
            <div className="text-muted-foreground">{loadoutSummary(preview.loadout)}</div>
            {hasBundle && (
              <p className="mt-1.5 flex items-start gap-1.5 text-[11.5px] text-emerald-500">
                <Package className="mt-px size-3.5 flex-none" />
                <span>
                  Includes a full asset bundle (~{humanSize(preview.bundle!.size)} from{" "}
                  {preview.bundle!.host}). Use <strong>Full import</strong> to download
                  and install everything — no mods needed first.
                </span>
              </p>
            )}
            {missing.length > 0 && !hasBundle && (
              <p className="mt-1.5 flex items-start gap-1.5 text-[11.5px] text-amber-500">
                <AlertTriangle className="mt-px size-3.5 flex-none" />
                <span>
                  Missing mods: {missing.map((s) => s.label).join(", ")}. Install
                  them for those parts to show.
                </span>
              </p>
            )}
          </div>
        )}
        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button
            variant={hasBundle ? "outline" : "default"}
            onClick={() => void onImport()}
            disabled={!preview || busy}
          >
            {busy && !phase ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Download className="size-4" />
            )}
            {hasBundle ? "Config only" : "Import"}
          </Button>
          {hasBundle && (
            <Button onClick={() => void onFullImport()} disabled={!preview || busy}>
              {busy && phase ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Package className="size-4" />
              )}
              {busy && phase ? phaseLabel(phase) : "Full import"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
