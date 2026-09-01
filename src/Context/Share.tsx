import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { ShareDialog, ImportShareDialog } from "../Components/Share/ShareDialogs";

/**
 * Sharing installed files, and the one place that owns its two dialogs.
 *
 * Sharing started in the Library because that is where files are listed, but nothing about
 * it is the Library's: Manage shows the same mods by rel, the Locker shows model and sound
 * sets, and a code pasted into the window belongs to no screen at all. Hoisting the dialogs
 * here is what lets each of those be one hook call instead of a third and fourth copy of the
 * same state, dialog and refresh wiring.
 *
 * Both entrances take whatever names a file: an absolute path (what the Library holds) or a
 * `mods/`-relative rel (what everything else uses). The backend resolves either — see
 * `src-tauri/src/fileshare.rs`.
 */
interface ShareContextValue {
  /** Open the share dialog for these paths or rels. */
  shareFiles: (paths: string[]) => void;
  /** Open the import dialog, optionally prefilled with a code. */
  importShare: (code?: string) => void;
}

const ShareContext = createContext<ShareContextValue | null>(null);

/**
 * A share code as it survives being pasted.
 *
 * Anchored at a word boundary rather than the start of the string, because a code arrives
 * surrounded by whatever it was posted with — "here you go: MXBS1-… enjoy". The payload is
 * base64, so it ends at the first character that isn't.
 */
const CODE_PATTERN = /\bMXBS1-[A-Za-z0-9+/=]+/;

/** Whether a paste is going somewhere that wants the text itself. */
function isEditable(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || typeof el.closest !== "function") return false;
  return Boolean(el.closest("input, textarea, [contenteditable=''], [contenteditable='true']"));
}

export function ShareProvider({
  onImported,
  children,
}: {
  /** Fired after an import installs something, so library views can re-scan. */
  onImported?: () => void;
  children: ReactNode;
}) {
  // Non-null while the share dialog is up: the paths it's about to pack.
  const [sharePaths, setSharePaths] = useState<string[] | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [importCode, setImportCode] = useState("");

  const shareFiles = useCallback((paths: string[]) => setSharePaths(paths), []);
  const importShare = useCallback((code = "") => {
    setImportCode(code);
    setImportOpen(true);
  }, []);

  const importRef = useRef(importShare);
  importRef.current = importShare;

  /**
   * A code pasted anywhere in the window opens the import dialog on it.
   *
   * The counterpart to dropping a file: a share arrives as text, so there is nothing to
   * drop — and hunting down Import → Paste a share code… for something already on the
   * clipboard is the step worth removing. A paste aimed at a field the user is typing in is
   * left alone, which is also what keeps this from fighting the import dialog's own box.
   */
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      if (isEditable(e.target)) return;
      const code = e.clipboardData?.getData("text")?.match(CODE_PATTERN)?.[0];
      if (code) importRef.current(code);
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, []);

  const onImportedRef = useRef(onImported);
  onImportedRef.current = onImported;

  const value = useMemo(() => ({ shareFiles, importShare }), [shareFiles, importShare]);

  return (
    <ShareContext.Provider value={value}>
      {children}
      <ShareDialog paths={sharePaths} onClose={() => setSharePaths(null)} />
      <ImportShareDialog
        open={importOpen}
        initialCode={importCode}
        onClose={() => setImportOpen(false)}
        onImported={() => {
          setImportOpen(false);
          onImportedRef.current?.();
        }}
      />
    </ShareContext.Provider>
  );
}

export function useShare(): ShareContextValue {
  const ctx = useContext(ShareContext);
  if (!ctx) throw new Error("useShare must be used inside a ShareProvider");
  return ctx;
}
