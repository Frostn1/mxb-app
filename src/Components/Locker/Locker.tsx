import { Component, type ReactNode, useEffect, useMemo, useState } from "react";
import { Bike, Palette, RotateCw, Hash, Info } from "lucide-react";
import { cn } from "@/lib/utils";
import { scanLibrary } from "../../api/mods";
import type { LibraryEntry } from "../../types";
import { displayName } from "../../lib/mods";
import LockerScene from "./LockerScene";
import { PRESET_LIVERIES, type Livery } from "./liveries";

/**
 * Locker — a high-end 3D preview of the rider's bike + livery.
 *
 * v1 renders a generic motocross model in a studio stage with a swappable,
 * texture-based livery. It lists the user's actually-installed bikes/liveries
 * so the tab reflects their library. Real per-bike models (community FBX → glTF)
 * and real decoded `.pnt` liveries drop into the same viewer once available —
 * see bikeModel.tsx / liveries.ts for the seams.
 */
export default function Locker() {
  const [bikes, setBikes] = useState<LibraryEntry[]>([]);
  const [paints, setPaints] = useState<LibraryEntry[]>([]);
  const [selectedBike, setSelectedBike] = useState<string | null>(null);
  const [livery, setLivery] = useState<Livery>(PRESET_LIVERIES[0]);
  const [installedPaint, setInstalledPaint] = useState(false);
  const [number, setNumber] = useState(21);
  const [autoRotate, setAutoRotate] = useState(true);

  // Pull the user's installed bikes + liveries so the locker mirrors their library.
  useEffect(() => {
    let cancelled = false;
    scanLibrary("mods/bikes")
      .then((entries) => {
        if (cancelled) return;
        setBikes(entries.filter((e) => e.category === "bike"));
        setPaints(entries.filter((e) => e.category === "bikePaint"));
      })
      .catch(() => {
        if (!cancelled) {
          setBikes([]);
          setPaints([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const bikePaints = useMemo(
    () =>
      selectedBike
        ? paints.filter(
            (p) => (p.parent ?? "").toLowerCase() === selectedBike.toLowerCase(),
          )
        : [],
    [paints, selectedBike],
  );

  const pickPreset = (l: Livery) => {
    setLivery(l);
    setInstalledPaint(false);
  };

  const pickInstalled = (entry: LibraryEntry) => {
    setLivery(liveryForInstalled(entry.name));
    setInstalledPaint(true);
  };

  return (
    <div className="flex h-full min-h-0">
      {/* Control rail */}
      <div className="flex w-[288px] flex-none flex-col overflow-y-auto border-r border-border bg-window px-4 pb-5 pt-4">
        <div className="mb-4">
          <h1 className="text-[15px] font-semibold text-foreground">Locker</h1>
          <p className="mt-0.5 text-[12px] leading-snug text-muted-foreground">
            Preview your bike and livery in 3D.
          </p>
        </div>

        {/* Bikes */}
        <Section icon={Bike} label="Your bikes">
          {bikes.length === 0 ? (
            <p className="px-1 text-[12px] text-faint">
              No installed bikes found. Install one from Browse to see it here.
            </p>
          ) : (
            <div className="flex flex-col gap-0.5">
              {bikes.map((b) => (
                <button
                  key={b.path}
                  onClick={() => setSelectedBike(b.name)}
                  className={cn(
                    "flex cursor-default items-center gap-2 rounded-md px-2.5 py-2 text-left text-[12.5px] transition-colors",
                    selectedBike === b.name
                      ? "bg-accent font-medium text-accent-foreground"
                      : "text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
                  )}
                >
                  <span className="truncate">{displayName(b.name)}</span>
                </button>
              ))}
            </div>
          )}
        </Section>

        {/* Liveries */}
        <Section icon={Palette} label="Livery">
          <div className="grid grid-cols-4 gap-1.5">
            {PRESET_LIVERIES.map((l) => (
              <button
                key={l.id}
                onClick={() => pickPreset(l)}
                title={l.name}
                className={cn(
                  "aspect-square cursor-default rounded-md border transition-all",
                  livery.id === l.id && !installedPaint
                    ? "border-primary ring-2 ring-primary/40"
                    : "border-white/10 hover:border-white/25",
                )}
                style={{
                  background: `linear-gradient(135deg, ${l.base} 55%, ${l.accent} 55%)`,
                }}
              />
            ))}
          </div>
          <p className="mt-2 px-0.5 text-[11.5px] text-muted-foreground">
            {livery.name}
          </p>

          {bikePaints.length > 0 && (
            <div className="mt-3 flex flex-col gap-0.5">
              <span className="px-0.5 pb-1 text-[10.5px] font-medium uppercase tracking-wide text-faint">
                Installed liveries
              </span>
              {bikePaints.map((p) => (
                <button
                  key={p.path}
                  onClick={() => pickInstalled(p)}
                  className={cn(
                    "cursor-default truncate rounded-md px-2.5 py-1.5 text-left text-[12px] transition-colors",
                    installedPaint && livery.name === displayName(p.name)
                      ? "bg-accent text-accent-foreground"
                      : "text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
                  )}
                >
                  {displayName(p.name)}
                </button>
              ))}
            </div>
          )}
        </Section>

        {/* Number */}
        <Section icon={Hash} label="Number">
          <input
            type="number"
            min={0}
            max={999}
            value={number}
            onChange={(e) =>
              setNumber(Math.max(0, Math.min(999, Number(e.target.value) || 0)))
            }
            className="w-20 rounded-md border border-input bg-background px-2.5 py-1.5 text-[13px] text-foreground focus:border-primary focus:outline-none"
          />
        </Section>

        {/* Controls */}
        <Section icon={RotateCw} label="Turntable">
          <label className="flex cursor-default items-center gap-2 text-[12.5px] text-muted-foreground">
            <input
              type="checkbox"
              checked={autoRotate}
              onChange={(e) => setAutoRotate(e.target.checked)}
              className="size-3.5 accent-[var(--primary)]"
            />
            Auto-rotate
          </label>
        </Section>

        {installedPaint && (
          <div className="mt-auto flex items-start gap-2 rounded-lg border border-warning/25 bg-warning/[0.06] px-3 py-2.5 text-[11.5px] leading-snug text-muted-foreground">
            <Info className="mt-0.5 size-3.5 flex-none text-warning" />
            <span>
              Showing a representative paint. Real installed liveries are
              encrypted (<code className="text-foreground/80">.pnt</code>) — exact
              decoding is in progress.
            </span>
          </div>
        )}
      </div>

      {/* Stage */}
      <div className="relative min-h-0 min-w-0 flex-1 overflow-hidden bg-[radial-gradient(circle_at_50%_35%,#20242b_0%,#0d0f12_70%)]">
        <div className="pointer-events-none absolute left-5 top-4 z-10">
          <div className="text-[13px] font-semibold text-foreground/90">
            {selectedBike ? displayName(selectedBike) : "Generic MX Bike"}
          </div>
          <div className="text-[11.5px] text-muted-foreground">{livery.name}</div>
        </div>
        <SceneBoundary>
          <LockerScene livery={livery} number={number} autoRotate={autoRotate} />
        </SceneBoundary>
      </div>
    </div>
  );
}

function Section({
  icon: Icon,
  label,
  children,
}: {
  icon: typeof Bike;
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="mb-5">
      <div className="mb-2 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wide text-faint">
        <Icon className="size-3.5" />
        {label}
      </div>
      {children}
    </div>
  );
}

/** Deterministically map an installed livery name to a representative preset. */
function liveryForInstalled(name: string): Livery {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  const base = PRESET_LIVERIES[h % PRESET_LIVERIES.length];
  return { ...base, id: `installed-${name}`, name: displayName(name) };
}

/** Contain any WebGL/context-init failure so the whole tab never white-screens. */
class SceneBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    if (this.state.failed) {
      return (
        <div className="flex h-full items-center justify-center px-8 text-center text-[13px] text-muted-foreground">
          3D preview couldn’t start — your system may not support WebGL in this
          window.
        </div>
      );
    }
    return this.props.children;
  }
}
