import { useEffect, useState } from "react";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import { coachReview, type ReviewOut, type SectionReview } from "@/api/coach";
import { gap, lapTime, lossColor, started } from "@/lib/format";
import Page, { Label } from "../Page";
import TrackMap from "./TrackMap";
import Traces from "./Traces";

/** One lap against the reference: where the time went, and what to change. */
export default function Review({ path, lap, onBack }: { path: string; lap: number; onBack: () => void }) {
  const t = useT();
  const [data, setData] = useState<ReviewOut | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [cursor, setCursor] = useState<number | null>(null);

  useEffect(() => {
    setData(null);
    setError(null);
    coachReview(path, lap)
      .then((r) => {
        setData(r);
        setSelected(r.review.focus[0] ?? null);
      })
      .catch((e) => setError(String(e)));
  }, [path, lap]);

  const back = t("review.back");
  if (error || !data) {
    return (
      <Page title={t("review.title")} onBack={onBack} backLabel={back}>
        <p className="text-[13px] text-muted-foreground">{error ?? t("common.loading")}</p>
      </Page>
    );
  }

  const { review, reference } = data;
  const total = (data.lap.timeMs - reference.timeMs) / 1000;

  return (
    <Page
      wide
      title={`${t("session.lap")} ${lap + 1} · ${lapTime(data.lap.timeMs)}`}
      sub={
        <>
          <span style={{ color: lossColor(total) }} className="font-mono">
            {gap(total)} s
          </span>{" "}
          {t("review.against")} {lapTime(reference.timeMs)} ({reference.bikeName}, {started(reference.started)})
        </>
      }
      onBack={onBack}
      backLabel={back}
    >
      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_400px]">
        <div className="min-w-0 space-y-4">
          <div className="h-[380px] border border-border bg-card p-3">
            <TrackMap review={review} selected={selected} cursor={cursor} onPick={setSelected} />
          </div>
          <div className="border border-border bg-card">
            <Traces review={review} selected={selected} cursor={cursor} onCursor={setCursor} />
          </div>
          <p className="text-[11.5px] text-faint">{t("review.legend")}</p>
        </div>

        <div className="space-y-6">
          <div>
            <Label>{t("review.focus")}</Label>
            {review.focus.length === 0 ? (
              <p className="border border-border px-4 py-3 text-[12.5px] text-muted-foreground">{t("review.nothing")}</p>
            ) : (
              <div className="space-y-2">
                {review.focus.map((i) => (
                  <SectionCard key={i} s={review.sections[i]} on={selected === i} onClick={() => setSelected(i)} open />
                ))}
              </div>
            )}
          </div>
          <div>
            <Label>{t("review.sections")}</Label>
            <div className="space-y-1">
              {review.sections.map((s, i) => (
                <SectionCard
                  key={i}
                  s={s}
                  on={selected === i}
                  onClick={() => setSelected(selected === i ? null : i)}
                  open={selected === i && !review.focus.includes(i)}
                />
              ))}
            </div>
          </div>
        </div>
      </div>
    </Page>
  );
}

function SectionCard({ s, on, open, onClick }: { s: SectionReview; on: boolean; open: boolean; onClick: () => void }) {
  return (
    <div className={cn("border bg-card", on ? "border-primary/60" : "border-border")}>
      <button onClick={onClick} className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left">
        <span className="text-[12.5px] font-semibold">{s.name}</span>
        <span className="font-mono text-[12px] tabular-nums" style={{ color: lossColor(s.lost) }}>
          {gap(s.lost)}
        </span>
      </button>
      {open && s.findings.length > 0 && (
        <ul className="space-y-2 border-t border-border px-3 py-2.5">
          {s.findings.map((f, k) => (
            <li key={k}>
              <div className="text-[12.5px] font-semibold">{f.title}</div>
              <div className="mt-0.5 text-[12px] leading-snug text-muted-foreground">{f.detail}</div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
