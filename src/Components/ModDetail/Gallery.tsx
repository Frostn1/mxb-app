import { useRef, useState } from "react";
import { Swiper, SwiperSlide } from "swiper/react";
import { Navigation, Pagination } from "swiper/modules";
import type { Swiper as SwiperClass } from "swiper";
import "swiper/css";
import "swiper/css/navigation";
import "swiper/css/pagination";
import { cn } from "@/lib/utils";
import { STRIP_THUMB_WIDTH } from "../../lib/imgcache";
import CachedImg from "@/Components/ui/cached-img";

interface GalleryProps {
  images: string[];
  /** Alt text, and the tooltip on each thumbnail. */
  title: string;
  /** Shown in place of the carousel when there are no images. */
  emptyLabel: string;
  /**
   * The frame's shape, which should match what the source actually publishes.
   *
   * mxb-mods posts 16:9 screenshots; mxbikes-shop's product images are square. Matching the
   * frame to the content is what lets the image fill it completely *and* stay uncropped —
   * `object-contain` in a mismatched frame is honest but leaves big empty bars.
   */
  aspect?: "video" | "square";
}

/**
 * The screenshot carousel plus its thumbnail strip.
 *
 * Extracted from `ModDetail` so the shop catalog's detail view can reuse it. Only the two
 * genuinely shared pieces moved: everything else in `ModDetail` is install machinery
 * (destinations, mirrors, the install dialog, the progress chain) that a paid item we never
 * download has no use for.
 */
export default function Gallery({
  images,
  title,
  emptyLabel,
  aspect = "video",
}: GalleryProps) {
  const frame = aspect === "square" ? "aspect-square" : "aspect-video";
  const swiperRef = useRef<SwiperClass | null>(null);
  const [active, setActive] = useState(0);

  // Opening a different mod must not leave the previous one's thumbnail highlighted — and
  // with a shorter gallery, an index carried over could point past the end. React's
  // documented "reset state when a prop changes" pattern, kept in here rather than relying
  // on every caller remembering to pass a `key`.
  const signature = images.join("|");
  const [seen, setSeen] = useState(signature);
  if (signature !== seen) {
    setSeen(signature);
    setActive(0);
    swiperRef.current?.slideTo(0, 0);
  }

  if (images.length === 0) {
    return (
      <div
        className={cn(
          "grid w-full flex-none place-items-center rounded-xl border border-border bg-gradient-to-br from-[#3a3f45] to-[#20242a] text-[13px] text-foreground/20",
          frame,
        )}
      >
        {emptyLabel}
      </div>
    );
  }

  return (
    <>
      <Swiper
        className={cn("frost-gallery w-full flex-none", frame)}
        modules={[Navigation, Pagination]}
        navigation
        pagination={{ clickable: true }}
        onSwiper={(s) => (swiperRef.current = s)}
        onSlideChange={(s) => setActive(s.activeIndex)}
      >
        {images.map((src) => (
          <SwiperSlide key={src}>
            {/* `contain`, not `cover`: the shop's product images are square while this frame
                is 16:9, and cropping to fill sliced the top and bottom off every one of them.
                A 16:9 screenshot still fills the frame exactly, so mxb-mods is unaffected —
                only images whose shape differs get letterboxed, which is the honest result. */}
            <CachedImg
              src={src}
              alt={title}
              className="size-full bg-[#15181c] object-contain"
            />
          </SwiperSlide>
        ))}
      </Swiper>
      {images.length > 1 && (
        <div className="flex flex-none gap-2 overflow-x-auto pb-1">
          {images.slice(0, 8).map((src, i) => (
            <button
              key={src}
              onClick={() => swiperRef.current?.slideTo(i)}
              className={cn(
                "w-24 flex-none overflow-hidden rounded-md border transition-opacity",
                frame,
                i === active
                  ? "border-primary"
                  : "border-transparent opacity-60 hover:opacity-100",
              )}
            >
              <CachedImg
                src={src}
                width={STRIP_THUMB_WIDTH}
                alt=""
                className="size-full bg-[#15181c] object-contain"
              />
            </button>
          ))}
        </div>
      )}
    </>
  );
}
