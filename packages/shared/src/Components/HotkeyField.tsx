import { useState } from "react";
import { toast } from "sonner";
import { useT } from "../i18n/context";
import { prettyHotkey } from "../lib/hotkey";
import { usePlatform } from "../lib/usePlatform";
import { cn } from "../lib/utils";

/** Turn a `KeyboardEvent.code` into the token Tauri's accelerator parser expects. */
function acceleratorKey(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return null;
}

/** Modifier-plus-key capture field for a global hotkey.
 *
 * A modifier is required: a bare `M` would be swallowed globally, including while the
 * player is typing a server chat message. */
export default function HotkeyField({
  value,
  onCapture,
  disabled,
}: {
  value: string;
  onCapture: (accelerator: string) => void;
  disabled?: boolean;
}) {
  const t = useT();
  const [recording, setRecording] = useState(false);
  const isMac = usePlatform() === "macos";

  const pretty = prettyHotkey(value, isMac);

  const onKeyDown = (e: React.KeyboardEvent) => {
    e.preventDefault();
    if (e.code === "Escape") {
      setRecording(false);
      return;
    }
    const key = acceleratorKey(e.code);
    if (!key) return; // a modifier on its own — keep waiting for the real key
    const mods: string[] = [];
    // Cmd on macOS and Ctrl elsewhere are the same accelerator token. The Windows key
    // is its own thing, so it must not be folded into it.
    if (e.ctrlKey || (isMac && e.metaKey)) mods.push("CommandOrControl");
    if (!isMac && e.metaKey) mods.push("Super");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    if (mods.length === 0) {
      toast.error(t("overlay.needModifier"), {
        description: t("overlay.needModifierDesc"),
      });
      return;
    }
    setRecording(false);
    onCapture([...mods, key].join("+"));
  };

  return (
    <button
      disabled={disabled}
      onClick={(e) => {
        // WebKit doesn't focus a button on click, and an unfocused button never sees
        // the keydown we're about to wait for.
        e.currentTarget.focus();
        setRecording(true);
      }}
      onBlur={() => setRecording(false)}
      onKeyDown={recording ? onKeyDown : undefined}
      className={cn(
        "min-w-[148px] cursor-default rounded-lg border px-3 py-1.5 text-center font-mono text-[12px] transition-colors disabled:opacity-50",
        recording
          ? "border-primary text-primary"
          : "border-border text-foreground/85 hover:bg-foreground/[0.05]",
      )}
    >
      {recording ? t("overlay.pressKeys") : pretty}
    </button>
  );
}
