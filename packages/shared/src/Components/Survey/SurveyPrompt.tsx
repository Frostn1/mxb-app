import { useCallback, useEffect, useRef, useState } from "react";
import { XIcon } from "lucide-react";
import { Button } from "../ui/button";
import { useI18n, useT, type BaseTKey, type TFunc } from "../../i18n/context";
import { cn } from "../../lib/utils";
import {
  pick,
  surveyAnswer,
  surveyDismiss,
  surveyDue,
  surveyShown,
  type Poll,
  type Prompt,
} from "../../api/survey";

/**
 * The survey prompt — one small card, rarely, with a question on it.
 *
 * Mounted once by every app. What it draws comes from the control plane
 * (`control-plane/src/survey.ts`); *whether* it draws anything at all is decided in Rust
 * (`crates/core/src/survey.rs`), which is the half that can see the config, how long the app
 * has been open and what this install has already been asked. This component polls, draws and
 * hands the answer back — it never decides that somebody should be interrupted.
 *
 * ## The two questions
 *
 * The first is the one that matters: a rating, one tap, sent the moment it is tapped. The
 * second — "what happened?" — is asked always after the answers a poll names (for the standing
 * mood poll, that is "bad": somebody who has just said it is going badly is the one person
 * worth asking why) and otherwise on a weighted coin, so it never becomes a form.
 *
 * Closing the card at the second question is not a dismissal. The rating is already in, and
 * treating a skipped follow-up as a refusal would snooze the prompt for somebody who had just
 * answered it.
 */

/** How often the backend is asked whether there is anything to show. */
const POLL_EVERY = 2 * 60 * 1000;

/** How long the thank-you sits there before the card goes. */
const THANKS_FOR = 2500;

/** What the card is showing. */
type Step = "ask" | "follow" | "thanks";

/**
 * The dictionary key for each id the backend can send, written out rather than built from the
 * id at the call site.
 *
 * A template key would typecheck against nothing, which is exactly the guarantee this project's
 * i18n layer exists to keep — a missing string should stop a build, not reach a player as a
 * bare key. An id this build has never heard of falls back to the id itself, which is a legible
 * label and not an empty button: the backend outlives the build that draws it.
 */
const MOOD_LABELS: Record<string, BaseTKey> = {
  bad: "survey.mood.bad",
  fine: "survey.mood.fine",
  good: "survey.mood.good",
};

const REASON_LABELS: Record<string, BaseTKey> = {
  crash: "survey.reason.crash",
  slow: "survey.reason.slow",
  confusing: "survey.reason.confusing",
  broken: "survey.reason.broken",
  missing: "survey.reason.missing",
  other: "survey.reason.other",
};

/** One of the lists above, or the id when this build doesn't know it. */
function label(t: TFunc<BaseTKey>, keys: Record<string, BaseTKey>, id: string): string {
  const key = keys[id];
  return key ? t(key) : id;
}

export default function SurveyPrompt() {
  const t = useT();
  const { resolved } = useI18n();
  const [prompt, setPrompt] = useState<Prompt | null>(null);
  const [step, setStep] = useState<Step>("ask");
  const [choice, setChoice] = useState<string>("");
  const [reasons, setReasons] = useState<string[]>([]);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  /** So a slow answer landing after the card closed can't reopen it. */
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  // Ask the backend, now and then. It says no almost every time; the cost of asking is one
  // command call that reads a config the app has already loaded.
  useEffect(() => {
    if (prompt) return;
    let stopped = false;
    const look = () => {
      surveyDue()
        .then((next) => {
          if (stopped || !next) return;
          setPrompt(next);
          setStep("ask");
          setChoice("");
          setReasons([]);
          setNote("");
          void surveyShown().catch(() => {});
        })
        .catch(() => {
          // A build without the commands, or a backend that said no. Either way there is
          // nothing to show and nothing worth telling the player about.
        });
    };
    look();
    const timer = setInterval(look, POLL_EVERY);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [prompt]);

  const close = useCallback(() => {
    if (live.current) setPrompt(null);
  }, []);

  const thank = useCallback(() => {
    if (!live.current) return;
    setStep("thanks");
    setTimeout(close, THANKS_FOR);
  }, [close]);

  if (!prompt) return null;
  const { poll } = prompt;

  /** The question, in the language on screen. */
  // `{{app}}` is an ambient variable, seeded per app from `VITE_APP_NAME`, so the standing
  // question names the product the player is actually looking at without being passed one.
  const ask = poll.kind === "mood" ? t("survey.mood.ask") : pick(poll.ask, resolved);

  /** The answers, as ids paired with what to draw for each. */
  const choices: { id: string; label: string }[] =
    poll.kind === "mood"
      ? prompt.moodChoices.map((id) => ({ id, label: label(t, MOOD_LABELS, id) }))
      : poll.choices.map((c) => ({ id: c.id, label: pick(c.label, resolved) }));

  /** The follow-up's chips: the poll's own, or the built-in list, which ships translated. */
  const chips: { id: string; label: string }[] =
    poll.reasons.length > 0
      ? poll.reasons.map((r) => ({ id: r.id, label: pick(r.label, resolved) }))
      : prompt.builtInReasons.map((id) => ({ id, label: label(t, REASON_LABELS, id) }));

  const answer = async (id: string) => {
    setBusy(true);
    setChoice(id);
    let more = false;
    try {
      more = (await surveyAnswer(poll.id, id)).followUp;
    } catch {
      // The rating is recorded on the Rust side before anything is sent, so a failed send is
      // an answer that didn't reach the endpoint rather than one the player has to repeat.
    }
    if (!live.current) return;
    setBusy(false);
    if (more) setStep("follow");
    else thank();
  };

  const send = async () => {
    setBusy(true);
    try {
      await surveyAnswer(poll.id, choice, reasons, note.trim() || null);
    } catch {
      /* as above */
    }
    if (!live.current) return;
    setBusy(false);
    thank();
  };

  /** The X. A dismissal only while the first question is still up — see the doc comment. */
  const dismiss = () => {
    if (step === "ask") void surveyDismiss().catch(() => {});
    close();
  };

  return (
    <div
      role="dialog"
      aria-label={ask}
      className="fixed bottom-4 right-4 z-50 w-[min(22rem,calc(100vw-2rem))] rounded-lg border bg-card p-4 text-card-foreground shadow-lg"
    >
      <button
        type="button"
        onClick={dismiss}
        aria-label={t("common.close")}
        className="absolute right-2 top-2 rounded-md p-1 text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
      >
        <XIcon className="size-4" />
      </button>

      {step === "thanks" ? (
        <p className="py-2 pr-6 text-[15px]">{t("survey.thanks")}</p>
      ) : step === "ask" ? (
        <>
          <p className="font-cond text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
            {t("survey.eyebrow")}
          </p>
          <p className="mt-1 pr-6 text-[15px] font-semibold">{ask}</p>
          <div className="mt-3 flex flex-wrap gap-2">
            {choices.map((c) => (
              <Button key={c.id} size="sm" variant="secondary" disabled={busy} onClick={() => void answer(c.id)}>
                {c.label}
              </Button>
            ))}
          </div>
        </>
      ) : (
        <>
          <p className="pr-6 text-[15px] font-semibold">{followUpAsk(poll, choice, t)}</p>
          <div className="mt-3 flex flex-wrap gap-1.5">
            {chips.map((c) => {
              const on = reasons.includes(c.id);
              return (
                <button
                  key={c.id}
                  type="button"
                  aria-pressed={on}
                  onClick={() =>
                    setReasons((was) => (on ? was.filter((r) => r !== c.id) : [...was, c.id]))
                  }
                  className={cn(
                    "rounded-full border px-3 py-1 text-[13px] transition-colors",
                    on
                      ? "border-primary bg-primary text-primary-foreground"
                      : "border-input text-foreground hover:bg-foreground/[0.06]",
                  )}
                >
                  {c.label}
                </button>
              );
            })}
          </div>

          {poll.note && (
            <div className="mt-3">
              <textarea
                value={note}
                onChange={(e) => setNote(e.target.value)}
                // The cap is enforced in Rust and again at the endpoint; this one is so the
                // box stops accepting rather than silently truncating what was typed.
                maxLength={280}
                rows={2}
                placeholder={t("survey.note.placeholder")}
                className="w-full resize-none rounded-md border border-input bg-background px-2.5 py-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              />
              {/* Said plainly, because it is the one thing here that travels as typed. */}
              <p className="mt-1 text-[11px] leading-snug text-muted-foreground">{t("survey.note.hint")}</p>
            </div>
          )}

          <div className="mt-3 flex items-center justify-end gap-2">
            <Button size="sm" variant="ghost" disabled={busy} onClick={close}>
              {t("survey.skip")}
            </Button>
            <Button size="sm" disabled={busy} onClick={() => void send()}>
              {t("survey.send")}
            </Button>
          </div>
        </>
      )}
    </div>
  );
}

/**
 * What the second question says.
 *
 * "What happened?" only makes sense after an answer the poll singled out — asking it of
 * somebody who just said things were good reads as though the app had not listened.
 */
function followUpAsk(poll: Poll, choice: string, t: TFunc<BaseTKey>): string {
  return poll.followUp.includes(choice)
    ? t("survey.followUp.whatHappened")
    : t("survey.followUp.anythingElse");
}
