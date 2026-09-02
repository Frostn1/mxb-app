import {
  AlertTriangle,
  CloudOff,
  Download,
  FolderInput,
  Loader2,
  OctagonAlert,
  X,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Button } from "@/Components/ui/button";
import { RUNTIME_NAME_KEY, onModsDehydrated } from "@/api/mods";
import type { ModsDehydrated } from "@/types";
import { useFrostmod } from "@/Context/FrostmodContext";
import { Trans } from "@/i18n";
import { useT } from "@/i18n/context";

/**
 * Slim bar for the two things that stop FrostMod reaching the game and can't be fixed
 * behind the player's back.
 *
 * **A stray `msvcr90.dll`** beside the game exe, which aborts MX Bikes with R6034 the
 * moment anything plain-imports the CRT. The app deletes the copies it planted itself, but
 * a file it can't prove it planted is somebody else's, and moving it aside has to be a
 * press. This one is red and outranks the other: it is an active crash, not a gap.
 *
 * **A missing Visual C++ runtime**, which needs admin rights to install, so it likewise has
 * to be a button. It earns a bar rather than living only in Settings because the symptom —
 * FrostMod silently failing to attach, or a bare "…dll was not found" box over the game —
 * gives no hint that Settings is where to look.
 *
 * **A mods folder inside a cloud sync tool**, which has no button at all — the fix is to
 * move the folder, which only the player can do. It earns a place here because its symptom
 * is the least legible of the three: the game sits on a black screen reading a tree it can't
 * read quickly, and nothing anywhere says that's what is happening.
 *
 * Renders nothing when none applies, which is the overwhelmingly common case.
 */
export default function RuntimeBanner() {
  const t = useT();
  const {
    runtimeWarning,
    installingRuntime,
    installRuntime,
    dismissRuntimeWarning,
    strayWarning,
    clearingStray,
    clearStrayMsvcr90,
  } = useFrostmod();
  const [cloud, setCloud] = useState<ModsDehydrated | null>(null);
  const [cloudDismissed, setCloudDismissed] = useState(false);

  // Announced once per game session by the backend, so a player who moves the folder stops
  // hearing about it from the next session on without anything having to invalidate here.
  useEffect(() => {
    const stop = onModsDehydrated((info) => {
      setCloud(info);
      setCloudDismissed(false);
    });
    return () => void stop.then((off) => off());
  }, []);

  if (strayWarning) {
    // `locked` is ours and the game is holding it open, so the fix is theirs to make in
    // the right order — pressing again before closing the game just fails again.
    const isLocked = strayWarning === "locked";
    return (
      <Bar
        tone="danger"
        body={
          <Trans
            k={isLocked ? "runtime.strayLocked" : "runtime.strayForeign"}
            values={{
              what: <span className="font-semibold">msvcr90.dll</span>,
            }}
          />
        }
        pitch={t(isLocked ? "runtime.strayLockedPitch" : "runtime.strayPitch")}
        action={t("runtime.strayFix")}
        actionIcon={FolderInput}
        busyLabel={t("runtime.strayClearing")}
        busy={clearingStray}
        onAction={() => void clearStrayMsvcr90()}
        onDismiss={dismissRuntimeWarning}
        dismissLabel={t("runtime.dismiss")}
      />
    );
  }

  // Below the two runtime bars: those are things that stop FrostMod dead, this is a folder
  // that makes the game slow and fragile. Shown only when nothing louder is up.
  if (!runtimeWarning && cloud && !cloudDismissed) {
    const evicted = cloud.count > 0;
    const provider = cloud.provider ?? t("cloud.genericProvider");
    return (
      <Bar
        tone={evicted ? "danger" : "warning"}
        icon={CloudOff}
        body={
          <Trans
            k={evicted ? "cloud.evictedBody" : "cloud.slowBody"}
            values={{ what: <span className="font-semibold">{provider}</span> }}
          />
        }
        pitch={t(evicted ? "cloud.evictedPitch" : "cloud.slowPitch")}
        onDismiss={() => setCloudDismissed(true)}
        dismissLabel={t("runtime.dismiss")}
      />
    );
  }

  if (!runtimeWarning) return null;

  // vc90 is the *game's* runtime and vc140 is FrostMod's own, so they need different
  // sentences — "MX Bikes needs this" and "FrostMod needs this" aren't interchangeable
  // when one of the two is visibly working.
  const isGameRuntime = runtimeWarning === "vc90";
  const bodyKey = isGameRuntime ? "runtime.bannerGame" : "runtime.bannerFrostmod";
  // `vc140_x86` never reaches here — the backend keeps it out of `missingRuntimes` because
  // nothing we ship is 32-bit — but the lookup covers it rather than leaving a hole that
  // would render an empty name if that ever changed.
  const nameKey = RUNTIME_NAME_KEY[runtimeWarning];

  return (
    <Bar
      tone="warning"
      body={
        <Trans
          k={bodyKey}
          values={{ what: <span className="font-semibold">{t(nameKey)}</span> }}
        />
      }
      pitch={t("runtime.pitch")}
      action={t("runtime.fixIt")}
      actionIcon={Download}
      busyLabel={t("runtime.installing")}
      busy={installingRuntime}
      onAction={() => void installRuntime(runtimeWarning)}
      onDismiss={dismissRuntimeWarning}
      dismissLabel={t("runtime.dismiss")}
    />
  );
}

/**
 * The bar itself, so the two cases differ in their words and colour rather than in their
 * markup. `danger` is a file crashing the game right now; `warning` is something that
 * won't work when it's reached.
 */
function Bar({
  tone,
  body,
  pitch,
  icon,
  action,
  actionIcon: ActionIcon,
  busyLabel,
  busy,
  onAction,
  onDismiss,
  dismissLabel,
}: {
  tone: "danger" | "warning";
  body: ReactNode;
  pitch: string;
  /** Overrides the tone's default glyph. */
  icon?: typeof Download;
  /** Omitted for a bar with nothing to press — a fix only the player can carry out. */
  action?: string;
  actionIcon?: typeof Download;
  busyLabel?: string;
  busy?: boolean;
  onAction?: () => void;
  onDismiss: () => void;
  dismissLabel: string;
}) {
  const danger = tone === "danger";
  const Icon = icon ?? (danger ? OctagonAlert : AlertTriangle);
  return (
    <div
      className={`flex items-center gap-3 border-b px-4 py-2 text-sm text-foreground ${
        danger
          ? "border-red-500/25 bg-red-500/10"
          : "border-amber-500/25 bg-amber-500/10"
      }`}
    >
      <Icon className={`size-4 shrink-0 ${danger ? "text-red-500" : "text-amber-500"}`} />
      <span className="min-w-0 truncate">
        {body}
        <span className="ml-1 text-muted-foreground">{pitch}</span>
      </span>

      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        {action && ActionIcon && onAction && (
          <Button size="sm" onClick={onAction} disabled={busy}>
            {busy ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <ActionIcon className="size-3.5" />
            )}
            {busy ? busyLabel : action}
          </Button>
        )}
        <Button
          size="icon"
          variant="ghost"
          className="size-8"
          onClick={onDismiss}
          disabled={busy}
          aria-label={dismissLabel}
        >
          <X className="size-4" />
        </Button>
      </div>
    </div>
  );
}
