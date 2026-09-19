import { useCallback, useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { ChevronLeft, ChevronRight, Maximize2, X, type LucideIcon } from "lucide-react";
import { useT } from "@/i18n";
import { cn } from "@frost/shared/lib/utils";
import SmartImg from "./Img";

/** One number worth reading off the picture: a track's length, a version, a price. */
export interface Figure {
  label: string;
  value: string;
}

/**
 * The mod, shown.
 *
 * A 16:9 frame at the width the window allows, with the figures laid along its foot. The band
 * this replaces spent 330px on four short strings over a blurred blow-up of the very
 * screenshot beside it — the picture is the fact here, and the numbers ride on it rather than
 * taking a card of their own.
 */
export default function MediaPanel({
  images,
  title,
  figures = [],
  emptyIcon: Empty,
  emptyLabel,
  badge,
  fit = "contain",
}: {
  images: string[];
  title: string;
  /** Laid over the foot of the picture. Anything without a value is dropped by the caller. */
  figures?: Figure[];
  emptyIcon?: LucideIcon;
  emptyLabel?: string;
  /** A corner note on the picture — "Locked", say. */
  badge?: ReactNode;
  /** `cover` for 16:9 screenshots, `contain` for square store art that would otherwise crop. */
  fit?: "cover" | "contain";
}) {
  const [idx, setIdx] = useState(0);
  const [zoom, setZoom] = useState(false);

  // Opening a different mod must not leave the previous one's thumbnail selected, and with a
  // shorter gallery the carried index could point past the end. React's documented "reset
  // state when a prop changes" pattern, kept here so no caller has to remember a `key`.
  const signature = images.join("|");
  const [seen, setSeen] = useState(signature);
  if (signature !== seen) {
    setSeen(signature);
    setIdx(0);
  }

  const shot = images[Math.min(idx, Math.max(0, images.length - 1))];

  return (
    <div className="flex min-w-0 flex-col gap-2.5">
      <div className="relative aspect-video w-full overflow-hidden rounded-xl border border-border bg-black/40">
        {shot ? (
          <button
            onClick={() => setZoom(true)}
            aria-label={title}
            className="group size-full cursor-default"
          >
            <SmartImg
              src={shot}
              width={1280}
              alt={title}
              className={cn("size-full", fit === "cover" ? "object-cover" : "object-contain")}
            />
            <span className="absolute right-3 top-3 grid size-7 place-items-center rounded-lg border border-white/25 bg-black/45 text-white/85 opacity-0 transition-opacity group-hover:opacity-100">
              <Maximize2 className="size-3.5" />
            </span>
          </button>
        ) : (
          <div className="grid size-full place-items-center bg-gradient-to-br from-[#25282d] to-[#131518] text-foreground/20">
            {Empty ? (
              <Empty className="size-9" strokeWidth={1.25} />
            ) : (
              <span className="font-cond text-[12.5px]">{emptyLabel}</span>
            )}
          </div>
        )}

        {badge && (
          <span className="absolute left-3 top-3 flex items-center gap-1 rounded-lg bg-black/60 px-2 py-1 font-cond text-[11px] text-white/85">
            {badge}
          </span>
        )}

        {/* The figures ride on the picture's foot. The scrim is only as tall as they are, so
            none of it washes over the part of the shot anyone is looking at. */}
        {figures.length > 0 && (
          <div className="pointer-events-none absolute inset-x-0 bottom-0 flex flex-wrap items-end gap-x-7 gap-y-1.5 bg-gradient-to-t from-[rgba(0,0,0,0.88)] via-[rgba(0,0,0,0.55)] to-transparent px-4 pb-3 pt-10">
            {figures.map((f) => (
              <span key={f.label} className="flex min-w-0 flex-col">
                <span className="font-cond text-[9.5px] font-semibold uppercase tracking-[0.16em] text-white/50">
                  {f.label}
                </span>
                <span className="truncate font-cond text-[14px] font-bold tabular-figures tracking-[-0.03em] text-white">
                  {f.value}
                </span>
              </span>
            ))}
          </div>
        )}
      </div>

      {images.length > 1 && (
        <div className="flex flex-none gap-2 overflow-x-auto pb-1">
          {images.map((img, i) => (
            <button
              key={img}
              onClick={() => setIdx(i)}
              className={cn(
                "h-[56px] w-[96px] flex-none cursor-default overflow-hidden rounded-lg bg-card transition-opacity",
                i === idx
                  ? "outline outline-2 -outline-offset-2 outline-primary"
                  : "opacity-55 hover:opacity-100",
              )}
            >
              <SmartImg src={img} width={240} alt="" className="size-full object-cover" />
            </button>
          ))}
        </div>
      )}

      {zoom && shot && (
        <Lightbox
          images={images}
          index={Math.min(idx, images.length - 1)}
          onIndex={setIdx}
          onClose={() => setZoom(false)}
          title={title}
        />
      )}
    </div>
  );
}

/**
 * One screenshot at the size the window allows.
 *
 * Arrow keys walk the set, Escape and a click anywhere off the picture leave.
 */
export function Lightbox({
  images,
  index,
  onIndex,
  onClose,
  title,
}: {
  images: string[];
  index: number;
  onIndex: (i: number) => void;
  onClose: () => void;
  title: string;
}) {
  const t = useT();
  const step = useCallback(
    (d: number) => onIndex((index + d + images.length) % images.length),
    [index, images.length, onIndex],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowRight") step(1);
      else if (e.key === "ArrowLeft") step(-1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, step]);

  // Portalled: the page sits inside a clipped column, and a viewer that covers the window
  // has to be a child of the window.
  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/92 px-16 py-12"
      onClick={onClose}
    >
      <SmartImg
        src={images[index]}
        alt={title}
        className="max-h-full max-w-full rounded-lg object-contain"
        onClick={(e) => e.stopPropagation()}
      />

      <button
        onClick={onClose}
        aria-label={t("common.close")}
        title={t("common.close")}
        className="absolute right-5 top-5 grid size-9 cursor-default place-items-center rounded-lg border border-white/20 bg-black/50 text-white/75 transition-colors hover:text-white"
      >
        <X className="size-4" />
      </button>

      {images.length > 1 && (
        <>
          <button
            onClick={(e) => {
              e.stopPropagation();
              step(-1);
            }}
            aria-label={t("common.back")}
            className="absolute left-4 top-1/2 grid size-10 -translate-y-1/2 cursor-default place-items-center rounded-full border border-white/20 bg-black/50 text-white/75 transition-colors hover:text-white"
          >
            <ChevronLeft className="size-5" />
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              step(1);
            }}
            aria-label={t("common.next")}
            className="absolute right-4 top-1/2 grid size-10 -translate-y-1/2 cursor-default place-items-center rounded-full border border-white/20 bg-black/50 text-white/75 transition-colors hover:text-white"
          >
            <ChevronRight className="size-5" />
          </button>
          <span className="absolute inset-x-0 bottom-5 text-center font-cond text-[12px] tabular-figures text-white/55">
            {index + 1} / {images.length}
          </span>
        </>
      )}
    </div>,
    document.body,
  );
}
