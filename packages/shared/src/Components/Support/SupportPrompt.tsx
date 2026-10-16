import { useEffect, useRef, useState } from "react";
import { Coffee } from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Button } from "../ui/button";
import { useT } from "../../i18n/context";
import {
  SUPPORT_MOMENT,
  SUPPORT_URL,
  supportDue,
  supportLater,
  supportNever,
  supportOpened,
  supportShown,
} from "../../api/support";

/**
 * The "support the developer" card — the survey card's look, with a heading, one line and
 * three buttons.
 *
 * Mounted once by the manager and the studio. When it may appear is decided in Rust
 * (`crates/core/src/support.rs`): after a success moment, or every tenth launch, never on the
 * first, at most once a week. This side only listens for those moments and draws the card.
 */

/** How long after a success moment the card waits, so it never lands on top of the toast. */
const AFTER_SUCCESS = 4000;

/** How long the app has been open before a launch alone is checked. Rust holds the same floor. */
const AFTER_LAUNCH = 3.5 * 60 * 1000;

export default function SupportPrompt() {
  const t = useT();
  const [open, setOpen] = useState(false);
  /** Mirrors `open` for the listeners, which outlive any one render. */
  const openRef = useRef(false);

  useEffect(() => {
    let stopped = false;
    let pending: number | undefined;

    const look = (trigger: "success" | "launch") => {
      if (openRef.current) return;
      supportDue(trigger)
        .then((due) => {
          if (stopped || !due || openRef.current) return;
          openRef.current = true;
          setOpen(true);
          void supportShown().catch(() => {});
        })
        .catch(() => {
          // A build without the commands. Nothing to show, nothing worth reporting.
        });
    };

    const onMoment = () => {
      window.clearTimeout(pending);
      pending = window.setTimeout(() => look("success"), AFTER_SUCCESS);
    };

    window.addEventListener(SUPPORT_MOMENT, onMoment);
    const launch = window.setTimeout(() => look("launch"), AFTER_LAUNCH);
    return () => {
      stopped = true;
      window.removeEventListener(SUPPORT_MOMENT, onMoment);
      window.clearTimeout(pending);
      window.clearTimeout(launch);
    };
  }, []);

  if (!open) return null;

  const close = () => {
    openRef.current = false;
    setOpen(false);
  };
  const support = () => {
    void openUrl(SUPPORT_URL).catch(() => {});
    void supportOpened().catch(() => {});
    close();
  };
  const later = () => {
    void supportLater().catch(() => {});
    close();
  };
  const never = () => {
    void supportNever().catch(() => {});
    close();
  };

  return (
    <div
      role="dialog"
      aria-label={t("support.title")}
      className="fixed bottom-4 right-4 z-50 w-[min(22rem,calc(100vw-2rem))] rounded-lg border bg-card p-4 text-card-foreground shadow-lg"
    >
      <p className="text-[15px] font-semibold">{t("support.title")}</p>
      <p className="mt-1 text-[13px] text-muted-foreground">{t("support.body")}</p>
      <div className="mt-3 flex flex-wrap gap-2">
        <Button size="sm" onClick={support}>
          <Coffee className="size-3.5" /> {t("support.support")}
        </Button>
        <Button size="sm" variant="secondary" onClick={later}>
          {t("support.later")}
        </Button>
        <Button size="sm" variant="ghost" onClick={never}>
          {t("support.never")}
        </Button>
      </div>
    </div>
  );
}
