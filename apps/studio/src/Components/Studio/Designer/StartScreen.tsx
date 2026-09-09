import { useEffect, useState } from "react";
import { Button } from "@frost/shared/Components/ui/button";
import {
  designerRecentForget,
  designerRecents,
  type RecentPaint,
} from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import { relativeTime } from "./relativeTime";
import { PaintDestCard, type PaintDestState } from "../paintDest";

/**
 * The Designer's front door, shaped like an IDE's.
 *
 * What it replaces: three buttons on a canvas nobody reached, because a destination was
 * picked for you and its sheets filled themselves in before you had decided anything. The
 * two reasons to open this tab are to start something and to carry on with something, so
 * those are the two columns — what you can do on the left with the app's name over them,
 * what you have already done on the right.
 *
 * It owns the window while it is up. A start screen framed by the editor's own panels is a
 * dialog wearing a page's clothes, and the panels have nothing to show until there is a
 * paint open anyway.
 */
export default function StartScreen({
  dest,
  busy,
  onBlank,
  onFromPaint,
  onFromPsd,
  onOpenRecent,
}: {
  dest: PaintDestState;
  busy: boolean;
  onBlank: () => void;
  onFromPaint: () => void;
  onFromPsd: () => void;
  onOpenRecent: (r: RecentPaint) => void;
}) {
  const t = useT();
  const [recent, setRecent] = useState<RecentPaint[] | null>(null);

  useEffect(() => {
    designerRecents()
      .then((rs) => {
        setRecent(rs);
      })
      .catch(() => setRecent([]));
  }, []);

  const forget = (path: string) => {
    setRecent((rs) => (rs ?? []).filter((r) => r.path !== path));
    void designerRecentForget(path).catch(() => {});
  };

  const shown = recent ?? [];

  return (
    <div className="absolute inset-0 flex bg-background">
      {/* ── What you can open ───────────────────────────────────────────────── */}
      <div className="flex w-[320px] shrink-0 flex-col gap-7 overflow-y-auto border-r border-border px-6 py-10">
        <div className="flex flex-col gap-1">
          <Heading>{t("designer.startTitle")}</Heading>
          <Action label={t("designer.startFromPaint")} disabled={busy} onClick={onFromPaint} />
          <Action label={t("designer.startFromPsd")} disabled={busy} onClick={onFromPsd} />
        </div>

        <div className="flex min-h-0 flex-col gap-1">
          <Heading>{t("designer.recent")}</Heading>
          {recent === null ? null : shown.length ? (
            shown.map((r) => (
              <div
                key={r.path}
                className="group flex items-center gap-2 rounded-md px-3 py-1.5 transition-colors hover:bg-foreground/[0.05]"
              >
                <button
                  type="button"
                  onClick={() => onOpenRecent(r)}
                  disabled={busy}
                  className="flex min-w-0 flex-1 cursor-default flex-col items-start text-left"
                >
                  <span className="max-w-full truncate text-[13px] text-foreground">{r.name}</span>
                  <span className="max-w-full truncate text-[11px] text-muted-foreground">
                    {r.model} · {relativeTime(r.savedAt)}
                  </span>
                </button>
                <button
                  type="button"
                  onClick={() => forget(r.path)}
                  title={t("designer.forget")}
                  className="flex-none rounded px-1 text-[12px] text-faint opacity-0 transition-opacity hover:text-foreground group-hover:opacity-100"
                >
                  ✕
                </button>
              </div>
            ))
          ) : (
            <p className="px-3 py-1 text-[12.5px] text-faint">{t("designer.noRecent")}</p>
          )}
        </div>
      </div>

      {/* ── Or start something ──────────────────────────────────────────────── */}
      <div className="flex min-w-0 flex-1 flex-col overflow-y-auto px-9 py-10">
        <Heading>{t("designer.startBlank")}</Heading>
        {/* The card carries its own padding for the popover it usually lives in; here the
            column already has some. */}
        <div className="mt-3 [&>div]:p-0">
          <PaintDestCard state={dest} bare />
        </div>
        <div className="mt-6 flex-none">
          <Button disabled={busy || !dest.model} onClick={onBlank}>
            {t("designer.createPaint")}
          </Button>
        </div>
      </div>
    </div>
  );
}

/** A section label — the same weight as an IDE welcome screen's. */
function Heading({ children }: { children: React.ReactNode }) {
  return (
    <span className="mb-1 px-3 text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
      {children}
    </span>
  );
}

/** One line in the left column — a verb, not a button on a form. */
function Action({
  label,
  disabled,
  onClick,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="cursor-default rounded-md px-3 py-1.5 text-left text-[13px] text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground disabled:opacity-40"
    >
      {label}
    </button>
  );
}
