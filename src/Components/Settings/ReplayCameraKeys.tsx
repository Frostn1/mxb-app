import { useCallback, useEffect, useState } from "react";
import { RotateCcw, TriangleAlert } from "lucide-react";
import { toast } from "sonner";
import {
  frostmodDefaultKeybinds,
  frostmodKeybinds,
  setFrostmodKeybinds,
  type FrostmodKeybind,
} from "../../api/mods";
import { useI18n } from "../../i18n/context";
import type { TKey } from "../../i18n/core";
import { Button } from "@/Components/ui/button";
import { cn } from "@/lib/utils";

/** Order and ids mirror FrostMod's `kb::Actions()`; the labels are ours to translate. */
const ACTION_LABELS: Record<string, TKey> = {
  setkey: "settings.rcamActionSetkey",
  delete: "settings.rcamActionDelete",
  clear: "settings.rcamActionClear",
  play: "settings.rcamActionPlay",
  prev: "settings.rcamActionPrev",
  next: "settings.rcamActionNext",
  save: "settings.rcamActionSave",
  load: "settings.rcamActionLoad",
  clean: "settings.rcamActionClean",
};

/** `KeyboardEvent.code` -> the name FrostMod parses. Physical codes on purpose: the game
 *  reads scancodes too, so what matters is the key you actually pressed, not what the
 *  current layout prints on it. Anything not here has no FrostMod name and is refused. */
const CODE_NAMES: Record<string, string> = {
  NumpadMultiply: "NumpadMultiply",
  NumpadAdd: "NumpadAdd",
  NumpadSubtract: "NumpadSubtract",
  NumpadDecimal: "NumpadDecimal",
  NumpadDivide: "NumpadDivide",
  Comma: "Comma",
  Period: "Period",
  Semicolon: "Semicolon",
  Slash: "Slash",
  Backquote: "Backtick",
  Minus: "Minus",
  Equal: "Equals",
  BracketLeft: "LeftBracket",
  Backslash: "Backslash",
  BracketRight: "RightBracket",
  Quote: "Quote",
  Space: "Space",
  Tab: "Tab",
  Enter: "Enter",
  Insert: "Insert",
  Delete: "Delete",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowLeft: "Left",
  ArrowUp: "Up",
  ArrowRight: "Right",
  ArrowDown: "Down",
};

function keyNameFor(e: KeyboardEvent): string | null {
  const c = e.code;
  if (/^Key[A-Z]$/.test(c)) return c.slice(3);
  if (/^Digit[0-9]$/.test(c)) return c.slice(5);
  if (/^Numpad[0-9]$/.test(c)) return c;
  if (/^F([1-9]|1[0-6])$/.test(c)) return c;
  return CODE_NAMES[c] ?? null;
}

function bindTextFor(e: KeyboardEvent): string | null {
  const key = keyNameFor(e);
  if (!key) return null;
  return (
    (e.ctrlKey ? "Ctrl+" : "") +
    (e.altKey ? "Alt+" : "") +
    (e.shiftKey ? "Shift+" : "") +
    key
  );
}

/** The replay camera editor's keys.
 *
 *  This exists because of one collision: the editor polls the keyboard, and the game reads
 *  the same key on the same frame — so a player with `S` bound to move-backwards could not
 *  press `S` to save without also moving the camera. A modifier does not help, because the
 *  game does not care whether Ctrl was held. The only fix is moving the action onto a key
 *  the game does not use, which is what this panel is for, and why the hint below says so
 *  rather than leaving people to work it out.
 */
export function ReplayCameraKeys() {
  const { t } = useI18n();
  const [binds, setBinds] = useState<FrostmodKeybind[] | null>(null);
  const [capturing, setCapturing] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let alive = true;
    void frostmodKeybinds()
      .then((b) => alive && setBinds(b))
      .catch(() => alive && setBinds([]));
    return () => {
      alive = false;
    };
  }, []);

  const persist = useCallback(
    async (next: FrostmodKeybind[]) => {
      const prev = binds;
      setBinds(next); // optimistic: the press should feel like it landed
      setSaving(true);
      try {
        await setFrostmodKeybinds(next);
      } catch (e) {
        setBinds(prev); // put the old key back rather than showing one FrostMod rejected
        toast.error(t("settings.rcamSaveFailed"), { description: String(e) });
      } finally {
        setSaving(false);
      }
    },
    [binds, t],
  );

  // While capturing, the next key press becomes the binding. Escape backs out, Backspace
  // unbinds. Captured on the window in the capture phase so the key never reaches whatever
  // else is listening — including the button that started the capture.
  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      if (["Control", "Alt", "Shift", "Meta", "OS"].includes(e.key)) return; // a modifier alone is not a binding
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape") {
        setCapturing(null);
        return;
      }
      const next =
        e.code === "Backspace" ? "none" : bindTextFor(e);
      if (!next) {
        toast.error(t("settings.rcamUnsupportedKey"));
        return;
      }
      const id = capturing;
      setCapturing(null);
      void persist((binds ?? []).map((b) => (b.id === id ? { ...b, key: next } : b)));
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, binds, persist, t]);

  const reset = useCallback(async () => {
    try {
      await persist(await frostmodDefaultKeybinds());
    } catch (e) {
      toast.error(t("settings.rcamSaveFailed"), { description: String(e) });
    }
  }, [persist, t]);

  if (!binds || binds.length === 0) return null;

  const duplicated = new Set(
    binds
      .filter((b) => b.key !== "none")
      .map((b) => b.key)
      .filter((k, i, all) => all.indexOf(k) !== i),
  );

  return (
    <div className="rounded-lg border border-input bg-background px-3 py-2.5">
      <div className="flex items-start justify-between gap-3">
        <div className="flex flex-col">
          <span className="text-[12.5px] text-foreground/85">
            {t("settings.rcamKeys")}
          </span>
          <span className="text-[11px] leading-relaxed text-muted-foreground">
            {t("settings.rcamKeysDesc")}
          </span>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => void reset()}
          disabled={saving}
          title={t("settings.rcamReset")}
        >
          <RotateCcw className="size-3.5" /> {t("settings.rcamReset")}
        </Button>
      </div>

      <div className="mt-3 grid gap-1 sm:grid-cols-2">
        {binds.map((b) => {
          const isCapturing = capturing === b.id;
          const clashes = b.key !== "none" && duplicated.has(b.key);
          return (
            <div
              key={b.id}
              className="flex items-center justify-between gap-2 rounded-md px-1.5 py-1"
            >
              <span className="text-[12px] text-foreground/75">
                {ACTION_LABELS[b.id] ? t(ACTION_LABELS[b.id]) : b.id}
              </span>
              <Button
                variant="outline"
                size="sm"
                className={cn(
                  "h-7 min-w-[104px] font-mono text-[11.5px]",
                  isCapturing && "border-primary text-primary",
                  clashes && !isCapturing && "border-destructive text-destructive",
                )}
                onClick={() => setCapturing(isCapturing ? null : b.id)}
                title={clashes ? t("settings.rcamDuplicate") : undefined}
              >
                {isCapturing
                  ? t("settings.rcamPressKey")
                  : b.key === "none"
                    ? t("settings.rcamUnbound")
                    : b.key}
              </Button>
            </div>
          );
        })}
      </div>

      <p className="mt-2 text-[11px] text-muted-foreground">
        {capturing ? t("settings.rcamCaptureHint") : t("settings.rcamAppliesHint")}
      </p>

      {duplicated.size > 0 && (
        <p className="mt-1 flex items-center gap-1.5 text-[11px] text-destructive">
          <TriangleAlert className="size-3.5" /> {t("settings.rcamDuplicate")}
        </p>
      )}
    </div>
  );
}
