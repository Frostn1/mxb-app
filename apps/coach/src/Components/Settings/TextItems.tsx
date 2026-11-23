import { useCallback, useEffect, useRef, useState } from "react";
import { RotateCcw } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { Slider } from "@frost/shared/Components/ui/slider";
import { Switch } from "@frost/shared/Components/ui/switch";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import {
  coachSetTextItem,
  coachTextItems,
  type ItemAnchor,
  type ItemStyle,
  type TextItem,
  type TextItems,
} from "@/api/coach";
import { Section } from "./parts";

/** How often a drag is written while it goes on. The recorder re-reads `hud.ini` about once a
 *  second, so more would only be writes nobody sees. */
const DRAG_WRITE_MS = 200;
/** The screen the pixel boxes are worked out for: the layout is 16:9 and a fraction means the
 *  same place on any screen, so one reference is enough to give the numbers a feel. */
const REF_W = 1920;
const REF_H = 1080;

/** What each item says in the preview, as the plugin would. */
const SAMPLE: Record<TextItem["id"], string> = { jump: "DOUBLE", pace: "MORE SPEED", gear: "↑ 3", cue: "BRAKE" };

/** The edit that is written for an item as it stands. */
const edit = (i: TextItem) => ({
  id: i.id,
  on: i.on,
  style: i.style,
  size: i.size,
  placed: i.placed,
  x: i.x,
  y: i.y,
  anchor: i.anchor,
  place: i.place,
});

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

/** A 0–1 number box with the same position in pixels beside it, on a 1920 x 1080 screen. Either
 *  one can be typed in; a value that isn't a number leaves the position where it was. */
function PositionField({
  label,
  value,
  max,
  onChange,
}: {
  label: string;
  value: number;
  max: number;
  onChange: (v: number) => void;
}) {
  const t = useT();
  return (
    <div className="flex items-center gap-2">
      <span className="w-3 text-[12px] font-semibold text-muted-foreground">{label}</span>
      <Input
        type="number"
        aria-label={`${label} ${t("items.fraction")}`}
        className="h-8 w-[84px] font-mono text-[12px]"
        min={0}
        max={1}
        step={0.005}
        value={Number(value.toFixed(3))}
        onChange={(e) => {
          const v = e.target.valueAsNumber;
          if (Number.isFinite(v)) onChange(clamp01(v));
        }}
      />
      <Input
        type="number"
        aria-label={`${label} ${t("items.pixels")}`}
        className="h-8 w-[78px] font-mono text-[12px]"
        min={0}
        max={max}
        step={1}
        value={Math.round(value * max)}
        onChange={(e) => {
          const v = e.target.valueAsNumber;
          if (Number.isFinite(v)) onChange(clamp01(v / max));
        }}
      />
      <span className="text-[11px] text-faint">px</span>
    </div>
  );
}

/** The 16:9 screen with every item on it, to take hold of and drag. The numbers below follow. */
function Layout({
  items,
  active,
  onSelect,
  onMove,
  onDrop,
}: {
  items: TextItem[];
  active: TextItem["id"] | null;
  onSelect: (id: TextItem["id"]) => void;
  onMove: (id: TextItem["id"], x: number, y: number) => void;
  onDrop: (id: TextItem["id"]) => void;
}) {
  const t = useT();
  const box = useRef<HTMLDivElement>(null);
  /** The item being dragged and where, in the item's own x / y, the pointer took hold of it. */
  const held = useRef<{ id: TextItem["id"]; dx: number; dy: number } | null>(null);
  const at = (e: React.PointerEvent) => {
    const r = box.current?.getBoundingClientRect();
    return r ? { fx: (e.clientX - r.left) / r.width, fy: (e.clientY - r.top) / r.height } : null;
  };
  return (
    <div
      ref={box}
      data-testid="text-layout"
      className="relative aspect-video w-full select-none overflow-hidden rounded-lg border border-border bg-[#2b2a27] [container-type:inline-size]"
      style={{
        backgroundImage:
          "linear-gradient(to right, rgba(255,255,255,.06) 1px, transparent 1px), linear-gradient(to bottom, rgba(255,255,255,.06) 1px, transparent 1px)",
        backgroundSize: "10% 10%",
      }}
      onPointerMove={(e) => {
        const h = held.current;
        const p = at(e);
        if (!h || !p) return;
        onMove(h.id, clamp01(p.fx - h.dx), clamp01(p.fy - h.dy));
      }}
      onPointerUp={() => {
        const h = held.current;
        held.current = null;
        if (h) onDrop(h.id);
      }}
      onPointerCancel={() => {
        const h = held.current;
        held.current = null;
        if (h) onDrop(h.id);
      }}
    >
      {items.map((i) => {
        const k = i.size ?? 1;
        // A jump call on the line is where the lip is, not on the screen: shown, not movable.
        const movable = i.on && !(i.id === "jump" && i.place !== "screen");
        const shift = i.anchor === "left" ? "0%" : i.anchor === "right" ? "-100%" : "-50%";
        return (
          <button
            key={i.id}
            type="button"
            data-testid={`layout-${i.id}`}
            aria-label={i.label}
            title={movable ? i.label : i.on ? t("items.onLine") : i.label}
            onPointerDown={(e) => {
              onSelect(i.id);
              const p = at(e);
              if (!movable || !p) return;
              (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
              held.current = { id: i.id, dx: p.fx - i.x, dy: p.fy - i.y };
            }}
            className={cn(
              "absolute whitespace-nowrap rounded px-[0.6cqw] py-[0.2cqw] font-mono uppercase leading-none text-white",
              movable ? "cursor-grab active:cursor-grabbing" : "cursor-default",
              i.style === "bold" && "font-black",
              i.style === "italic" && "italic",
              i.on ? "bg-black/60" : "bg-black/20 opacity-40",
              !movable && i.on && "border border-dashed border-white/40",
              active === i.id ? "outline outline-2 outline-primary" : "outline outline-1 outline-white/20",
            )}
            style={{
              left: `${i.x * 100}%`,
              top: `${i.y * 100}%`,
              transform: `translateX(${shift})`,
              fontSize: `${1.55 * k}cqw`,
              touchAction: "none",
            }}
          >
            {SAMPLE[i.id]}
          </button>
        );
      })}
    </div>
  );
}

/** One item's controls: on or off, its style and size, where it is and which edge of it that is,
 *  and a reset to how the recorder draws it with none of this set. */
function ItemRow({
  item,
  textSize,
  active,
  onChange,
  onReset,
}: {
  item: TextItem;
  textSize: number;
  active: boolean;
  onChange: (next: TextItem, later?: boolean) => void;
  onReset: () => void;
}) {
  const t = useT();
  const size = item.size ?? (item.followsTextSize ? textSize : 1);
  const style = (s: ItemStyle) => onChange({ ...item, style: s });
  const move = (x: number, y: number) => onChange({ ...item, x, y, placed: true });
  const pristine =
    item.on && item.style === "default" && item.size === null && !item.placed && item.anchor === item.defaultAnchor &&
    (item.place === null || item.place === "line");
  return (
    <div
      data-testid={`item-${item.id}`}
      className={cn("flex flex-col gap-3 rounded-lg border p-3", active ? "border-primary/60" : "border-border")}
    >
      <div className="flex items-center gap-3">
        <Switch checked={item.on} onCheckedChange={(on) => onChange({ ...item, on })} aria-label={item.label} />
        <span className="flex-1 text-[12.5px] font-semibold text-foreground/90">{item.label}</span>
        <Button size="sm" variant="outline" disabled={pristine} onClick={onReset}>
          <RotateCcw className="size-3.5" />
          {t("items.reset")}
        </Button>
      </div>
      <div className={cn("flex flex-col gap-3", !item.on && "pointer-events-none opacity-50")}>
        {item.place !== null && (
          <div className="flex items-center justify-between gap-4">
            <span className="text-[12px] text-muted-foreground">{t("items.place")}</span>
            <Segmented
              size="sm"
              value={item.place}
              onChange={(place) => onChange({ ...item, place })}
              options={[
                { value: "line", label: t("items.placeLine") },
                { value: "screen", label: t("items.placeScreen") },
              ]}
            />
          </div>
        )}
        <div className="flex items-center justify-between gap-4">
          <span className="text-[12px] text-muted-foreground">{t("items.style")}</span>
          <Segmented
            size="sm"
            value={item.style}
            onChange={style}
            options={[
              { value: "default", label: t("items.styleDefault") },
              { value: "block", label: t("items.styleBlock") },
              { value: "bold", label: t("items.styleBold") },
              { value: "italic", label: t("items.styleItalic") },
            ]}
          />
        </div>
        <div className="flex items-center justify-between gap-4">
          <span className="text-[12px] text-muted-foreground">{t("items.size")}</span>
          <div className="flex w-[260px] items-center gap-3">
            <Slider
              className="flex-1"
              min={0.5}
              max={3}
              step={0.05}
              value={[size]}
              onValueChange={([v]) => onChange({ ...item, size: v }, true)}
              onValueCommit={([v]) => onChange({ ...item, size: v })}
            />
            <span className="w-12 text-right font-mono text-[12px] text-muted-foreground">{size.toFixed(2)}×</span>
          </div>
        </div>
        <div
          className={cn(
            "flex flex-wrap items-center justify-between gap-x-4 gap-y-2",
            item.place === "line" && "pointer-events-none opacity-50",
          )}
        >
          <span className="text-[12px] text-muted-foreground">
            {item.place === "line" ? t("items.positionLine") : t("items.position")}
          </span>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <PositionField label="X" value={item.x} max={REF_W} onChange={(x) => move(x, item.y)} />
            <PositionField label="Y" value={item.y} max={REF_H} onChange={(y) => move(item.x, y)} />
          </div>
        </div>
        <div className="flex items-center justify-between gap-4">
          <span className="text-[12px] text-muted-foreground">{t("items.anchor")}</span>
          <Segmented
            size="sm"
            value={item.anchor}
            onChange={(anchor: ItemAnchor) => onChange({ ...item, anchor })}
            options={[
              { value: "left", label: t("items.anchorLeft") },
              { value: "center", label: t("items.anchorCenter") },
              { value: "right", label: t("items.anchorRight") },
            ]}
          />
        </div>
        {!item.placed && item.place !== "line" && (
          <span className="text-[11.5px] text-muted-foreground">{t("items.defaultPlace")}</span>
        )}
      </div>
    </div>
  );
}

/** A style, size and place for each word the recorder draws, with a screen to drag them about on.
 *  Live: the recorder re-reads `hud.ini` about once a second. */
export default function TextItemsSettings() {
  const t = useT();
  const [state, setState] = useState<TextItems | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [active, setActive] = useState<TextItem["id"] | null>(null);
  /** The one edit waiting for the rider to stop (a slider being dragged), and its timer. */
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const waiting = useRef<TextItem | null>(null);
  /** A drag is going on: nothing from the file replaces what is on screen meanwhile. */
  const dragging = useRef(false);
  const lastWrite = useRef(0);
  /** The items as shown, kept in step at once (not at the next render) for a drag that moves
   *  faster than the renders. */
  const items = useRef<TextItem[]>([]);
  /** Writes go one after another, and only the answer to the last one is shown. */
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const sent = useRef(0);

  useEffect(() => {
    coachTextItems()
      .then((s) => {
        items.current = s.items;
        setState(s);
      })
      .catch((e) => setError(String(e)));
    return () => {
      // Leaving with a slider's edit still waiting: write it rather than lose it.
      if (timer.current) clearTimeout(timer.current);
      const w = waiting.current;
      if (w) void coachSetTextItem(edit(w)).catch(() => {});
    };
  }, []);

  const write = useCallback((item: TextItem, reset = false) => {
    lastWrite.current = Date.now();
    const mine = ++sent.current;
    queue.current = queue.current.then(() =>
      coachSetTextItem({ ...edit(item), reset })
        .then((s) => {
          // Only the newest answer, and not while the rider is mid-edit: anything else is older than what is shown.
          if (mine === sent.current && !dragging.current && !waiting.current) {
            items.current = s.items;
            setState(s);
          }
        })
        .catch((e) => setError(String(e))),
    );
    return queue.current;
  }, []);

  const patch = (id: TextItem["id"], f: (i: TextItem) => TextItem) => {
    items.current = items.current.map((i) => (i.id === id ? f(i) : i));
    setState((s) => (s ? { ...s, items: items.current } : s));
  };

  /** Write the edit that is waiting for its timer, now. */
  const flush = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    const w = waiting.current;
    waiting.current = null;
    if (w) void write(w);
  };

  /** Show `next` now; write it now, or once the rider has stopped moving it. */
  const save = (next: TextItem, later = false) => {
    // One timer serves one item: a different item's edit writes the first one's before its own.
    if (waiting.current && waiting.current.id !== next.id) flush();
    patch(next.id, () => next);
    if (!later) {
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
      waiting.current = null;
      void write(next);
      return;
    }
    if (timer.current) clearTimeout(timer.current);
    waiting.current = next;
    timer.current = setTimeout(flush, 250);
  };

  const reset = (item: TextItem) => {
    if (waiting.current?.id === item.id) {
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
      waiting.current = null;
    }
    const back: TextItem = {
      ...item,
      on: true,
      style: "default",
      size: null,
      placed: false,
      x: item.defaultX,
      y: item.defaultY,
      anchor: item.defaultAnchor,
      place: item.place === null ? null : "line",
    };
    patch(item.id, () => back);
    void write(item, true);
  };

  /** While a drag goes on: show it and write it now and then, so the game follows. */
  const drag = (id: TextItem["id"], x: number, y: number) => {
    dragging.current = true;
    setActive(id);
    const cur = items.current.find((i) => i.id === id);
    if (!cur) return;
    const moved = { ...cur, x, y, placed: true };
    patch(id, () => moved);
    if (Date.now() - lastWrite.current > DRAG_WRITE_MS) void write(moved);
  };
  const drop = (id: TextItem["id"]) => {
    dragging.current = false;
    const it = items.current.find((i) => i.id === id);
    if (it) void write(it);
  };

  return (
    <Section title={t("items.title")} desc={t("items.desc")}>
      {error && <p className="text-[12px] text-muted-foreground">{error}</p>}
      {!state && !error && <p className="text-[12px] text-muted-foreground">{t("common.loading")}</p>}
      {state && (
        <>
          {state.preItems && <p className="text-[12px] text-warning">{t("items.needs")}</p>}
          <Layout items={state.items} active={active} onSelect={setActive} onMove={drag} onDrop={drop} />
          <p className="text-[11.5px] text-muted-foreground">{t("items.layoutBody")}</p>
          <div className="flex flex-col gap-3">
            {state.items.map((i) => (
              <ItemRow
                key={i.id}
                item={i}
                textSize={state.textSize}
                active={active === i.id}
                onChange={save}
                onReset={() => reset(i)}
              />
            ))}
          </div>
        </>
      )}
    </Section>
  );
}
