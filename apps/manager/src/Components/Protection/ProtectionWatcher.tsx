import { useEffect, useRef } from "react";
import { toast } from "sonner";
import { useT } from "@/i18n";
import { useGameRunning } from "../../lib/useGameRunning";
import { caughtSince, fetchProtection, namesOf, nowS } from "../../lib/protection";

/**
 * The note after a session: when the game closes and FrostMod caught something while it ran,
 * say so, by name, with a way to the Crash protection page. Renders nothing.
 */
export default function ProtectionWatcher({ onOpen }: { onOpen: () => void }) {
  const t = useT();
  const { running } = useGameRunning();
  const startedAt = useRef<number | null>(null);
  useEffect(() => {
    if (running) {
      if (startedAt.current === null) startedAt.current = nowS();
      return;
    }
    const since = startedAt.current;
    startedAt.current = null;
    if (since === null) return;
    void fetchProtection().then(({ caught, table }) => {
      const session = caughtSince(caught, since);
      if (session.length === 0) return;
      toast.success(t("protection.sessionNote", { count: session.length }), {
        description: namesOf(table, session).join(", "),
        duration: 12_000,
        action: { label: t("protection.cardOpen"), onClick: onOpen },
      });
    });
  }, [running, t, onOpen]);
  return null;
}
