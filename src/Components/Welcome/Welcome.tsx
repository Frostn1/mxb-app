import { useState } from "react";
import { Snowflake, ArrowLeft, ArrowRight } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useT, type TKey } from "../../i18n/context";
import { Button } from "@/Components/ui/button";
import { cn } from "@/lib/utils";

interface WelcomeProps {
  /** Called when the user finishes or skips the tour. */
  onDone: () => void;
}

interface Slide {
  icon: LucideIcon;
  title: TKey;
  body: TKey;
}

// Just the intro. The per-feature walkthrough (Browse, Library, FrostMod, …) is
// handled by the in-app guided tour, which runs right after this — so the slides
// that used to duplicate it were dropped to avoid saying everything twice.
const SLIDES: Slide[] = [
  {
    icon: Snowflake,
    title: "welcome.intro.title",
    body: "welcome.intro.body",
  },
];

export default function Welcome({ onDone }: WelcomeProps) {
  const t = useT();
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
              {t(slide.title)}
            </h1>
            <p className="min-h-[72px] max-w-[400px] text-center text-[13.5px] leading-relaxed text-muted-foreground">
              {t(slide.body)}
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
            {t("welcome.getStarted")}
          </Button>
        ) : (
          <div className="flex w-full items-center justify-between gap-3">
            {index > 0 ? (
              <Button variant="ghost" onClick={back}>
                <ArrowLeft /> {t("common.back")}
              </Button>
            ) : (
              <Button variant="ghost" onClick={onDone}>
                {t("common.skip")}
              </Button>
            )}
            <Button onClick={next}>
              {isLast ? t("welcome.getStarted") : t("common.next")}
              {!isLast && <ArrowRight />}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
