import { useState } from "react";
import { Snowflake, ArrowLeft, ArrowRight } from "lucide-react";
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

// Just the intro. The per-feature walkthrough (Browse, Library, FrostMod, …) is
// handled by the in-app guided tour, which runs right after this — so the slides
// that used to duplicate it were dropped to avoid saying everything twice.
const SLIDES: Slide[] = [
  {
    icon: Snowflake,
    title: "Welcome to MXB App",
    body: "Your mod manager for MX Bikes. Keep your tracks, bikes and paints organized in one place — no more zip files scattered across your desktop. We'll show you around in a few seconds.",
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

        {SLIDES.length > 1 && (
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
        )}

        {SLIDES.length === 1 ? (
          <Button className="w-full" onClick={onDone}>
            Get started
          </Button>
        ) : (
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
        )}
      </div>
    </div>
  );
}
