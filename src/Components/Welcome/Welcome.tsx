import { useState } from "react";
import {
  Snowflake,
  Compass,
  Download,
  RefreshCw,
  ArrowLeft,
  ArrowRight,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/Components/ui/button";
import { cn } from "@/lib/utils";

interface WelcomeProps {
  /** Called when the user finishes or skips the tour. */
  onDone: () => void;
}

interface Slide {
  icon: LucideIcon;
  title: string;
  body: string;
}

const SLIDES: Slide[] = [
  {
    icon: Snowflake,
    title: "Welcome to MXB App",
    body: "Your mod manager for MX Bikes. Keep your tracks, bikes and paints organized in one place — no more zip files scattered across your desktop.",
  },
  {
    icon: Compass,
    title: "Browse & install",
    body: "Explore mxb-mods.com right inside the app and install any track, bike or paint with a single click. MXB App unpacks it straight into your MX Bikes folder.",
  },
  {
    icon: Download,
    title: "One click, done",
    body: "Downloads, extraction and placement are handled for you — the correct folder structure every time, so mods just work when you launch the game.",
  },
  {
    icon: RefreshCw,
    title: "FrostMod keeps it live",
    body: "FrostMod reloads the game after an install, so new content shows up without restarting MX Bikes. Set your MX Bikes folder next and you're ready to ride.",
  },
];

export default function Welcome({ onDone }: WelcomeProps) {
  const [index, setIndex] = useState(0);
  const slide = SLIDES[index];
  const Icon = slide.icon;
  const isLast = index === SLIDES.length - 1;

  const next = () => (isLast ? onDone() : setIndex((i) => i + 1));
  const back = () => setIndex((i) => Math.max(0, i - 1));

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-background/80 backdrop-blur-sm px-10">
      <div className="flex w-full max-w-[480px] flex-col items-center gap-8 rounded-2xl border border-input bg-card p-9 shadow-2xl">
        <div className="flex flex-col items-center gap-4">
          <div className="grid size-14 place-items-center rounded-[15px] bg-gradient-to-br from-[#9ccfec] to-[#5d8fb0] text-[#0d0f12]">
            <Icon className="size-7" strokeWidth={2.5} />
          </div>
          <div className="flex flex-col items-center gap-2">
            <h1 className="text-center text-[24px] font-extrabold tracking-[-0.4px]">
              {slide.title}
            </h1>
            <p className="min-h-[72px] max-w-[400px] text-center text-[13.5px] leading-relaxed text-muted-foreground">
              {slide.body}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {SLIDES.map((_, i) => (
            <span
              key={i}
              className={cn(
                "h-1.5 rounded-full transition-all",
                i === index ? "w-5 bg-primary" : "w-1.5 bg-foreground/20",
              )}
            />
          ))}
        </div>

        <div className="flex w-full items-center justify-between gap-3">
          {index > 0 ? (
            <Button variant="ghost" onClick={back}>
              <ArrowLeft /> Back
            </Button>
          ) : (
            <Button variant="ghost" onClick={onDone}>
              Skip
            </Button>
          )}
          <Button onClick={next}>
            {isLast ? "Get started" : "Next"}
            {!isLast && <ArrowRight />}
          </Button>
        </div>
      </div>
    </div>
  );
}
