import { useEffect, useRef, useState } from "react";
import { Upload as UploadIcon } from "lucide-react";
import { byteRate, byteSize } from "@/lib/format";
import { cancelUpload, dismissUpload, initUploads, isActive, percent, retryUpload, useUploads, type Upload } from "@/lib/uploads";
import { errorText } from "@/lib/api";
import { Button } from "./ui";

const label: Record<Upload["status"], string> = {
  checking: "Checking…",
  uploading: "Uploading",
  retrying: "Connection lost, retrying…",
  installing: "Installing on the server…",
  done: "Installed",
  error: "Failed",
  cancelled: "Cancelled",
};

/** One upload: name, bar, percent, speed, and the buttons that fit its state. */
export function UploadRow({ upload, showServer = true }: { upload: Upload; showServer?: boolean }) {
  const [problem, setProblem] = useState<string | null>(null);
  const active = isActive(upload);
  const pct = percent(upload);
  const act = (run: () => Promise<unknown>) => void run().catch((e) => setProblem(errorText(e)));
  return (
    <div className="flex flex-col gap-1.5 py-2.5" data-testid="upload-row">
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium" title={upload.fileName}>{upload.fileName}</div>
          <div className="truncate text-xs text-muted-foreground">
            {showServer && <>{upload.serverName} · </>}
            {label[upload.status]}
            {upload.status === "uploading" && <> · {pct}%{upload.speed > 0 && <> · {byteRate(upload.speed)}</>}</>}
            {upload.attempt > 1 && active && <> · attempt {upload.attempt}</>}
          </div>
        </div>
        {active && upload.status !== "installing" && <Button size="sm" variant="danger" onClick={() => act(() => cancelUpload(upload.id))}>Cancel</Button>}
        {(upload.status === "error" || upload.status === "cancelled") && <Button size="sm" onClick={() => act(() => retryUpload(upload))}>Retry</Button>}
        {!active && <Button size="sm" variant="ghost" onClick={() => act(() => dismissUpload(upload.id))}>Dismiss</Button>}
      </div>
      {active && (
        <div className="h-1.5 overflow-hidden rounded-full bg-secondary" role="progressbar" aria-label={`${upload.fileName} upload`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
          <div className={`h-full rounded-full bg-primary transition-[width] ${upload.status === "installing" || upload.status === "checking" ? "upload-progress-bar w-2/5" : ""}`} style={upload.status === "uploading" || upload.status === "retrying" ? { width: `${pct}%` } : undefined} />
        </div>
      )}
      {upload.status === "error" && upload.error && <p className="text-xs break-words text-destructive">{upload.error}</p>}
      {upload.status === "retrying" && upload.error && <p className="text-xs break-words text-muted-foreground">{upload.error}</p>}
      {problem && <p className="text-xs break-words text-destructive">{problem}</p>}
      {upload.status === "uploading" && <span className="sr-only">{byteSize(upload.sent)} of {byteSize(upload.bytes)}</span>}
    </div>
  );
}

/** The header's uploads button and its list, on every page. */
export function UploadsIndicator() {
  const uploads = useUploads();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => { void initUploads(); }, []);
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => { if (box.current && !box.current.contains(e.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open]);
  if (uploads.length === 0) return null;

  const running = uploads.filter(isActive);
  const failed = uploads.filter((u) => u.status === "error").length;
  const total = running.reduce((sum, u) => sum + u.bytes, 0);
  const sent = running.reduce((sum, u) => sum + u.sent, 0);
  const speed = running.reduce((sum, u) => sum + u.speed, 0);
  const summary = running.length > 0
    ? `${total > 0 ? Math.min(100, Math.floor((sent / total) * 100)) : 0}%${speed > 0 ? ` · ${byteRate(speed)}` : ""}`
    : failed > 0 ? "Failed" : "Done";

  return (
    <div ref={box} className="relative mr-2 shrink-0">
      <button type="button" onClick={() => setOpen((v) => !v)} aria-expanded={open} aria-label="Uploads" title="Uploads" className="flex h-9 items-center gap-2 rounded-md px-2.5 text-sm font-medium text-muted-foreground hover:bg-accent hover:text-foreground">
        <UploadIcon className={`size-4 ${running.length > 0 ? "text-primary" : failed > 0 ? "text-destructive" : ""}`} />
        <span className="tabular-nums">{running.length > 1 ? `${running.length} uploads · ` : ""}{summary}</span>
      </button>
      {open && (
        <div role="dialog" aria-label="Uploads" className="absolute right-0 top-11 z-50 w-96 max-w-[90vw] rounded-xl border bg-card p-3 text-card-foreground shadow-lg">
          <div className="divide-y">
            {uploads.map((u) => <UploadRow key={u.id} upload={u} />)}
          </div>
        </div>
      )}
    </div>
  );
}
