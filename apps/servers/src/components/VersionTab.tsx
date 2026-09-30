import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Download, Upload } from "lucide-react";
import { errorText, isLegacyStatus, serverReleaseInstall, serverReleasePreview, serverStatus, serverUpload, type ReleasePreview, type ServerView } from "@/lib/api";
import { Button, Card, ErrorLine, Notice } from "./ui";

const versionCache = new Map<string, { version: string; revision: string; buildId: string }>();

export function VersionTab({ server }: { server: ServerView }) {
  const [running, setRunning] = useState<{ version: string; revision: string; buildId: string } | null>(versionCache.get(server.id) ?? null);
  const [version, setVersion] = useState("");
  const [channel, setChannel] = useState<"stable" | "prerelease" | "tag">("stable");
  const [tag, setTag] = useState("");
  const [preview, setPreview] = useState<ReleasePreview | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const load = useCallback(async () => {
    const report = await serverStatus(server.id);
    if (report.status && !isLegacyStatus(report.status)) {
      const value = { version: report.status.version, revision: report.status.revision, buildId: report.status.build_id };
      versionCache.set(server.id, value);
      setRunning(value);
    }
  }, [server.id]);
  useEffect(() => { void load().catch((e) => setError(errorText(e))); }, [load]);

  const checkRelease = async () => {
    setBusy("Checking and verifying release…"); setError(null); setDone(null); setPreview(null);
    try { setPreview(await serverReleasePreview(channel, channel === "tag" ? tag.trim() : undefined)); }
    catch (e) { setError(errorText(e)); } finally { setBusy(null); }
  };
  const installRelease = async () => {
    if (!preview || !window.confirm(`Install ${preview.tag} on ${server.name}? The server will restart.`)) return;
    setBusy("Uploading, restarting, and verifying…"); setError(null); setDone(null);
    try { await serverReleaseInstall(server.id, preview); setDone(`${preview.tag} installed.`); setPreview(null); await load(); }
    catch (e) { setError(errorText(e)); } finally { setBusy(null); }
  };
  const manual = async () => {
    if (!version.trim()) return;
    const path = await open({ multiple: false, directory: false });
    if (!path || !window.confirm(`Install ${version.trim()} on ${server.name}?`)) return;
    setBusy("Uploading server…"); setError(null); setDone(null);
    try { await serverUpload(server.id, "version", path, version.trim()); setDone("Server updated."); await load(); }
    catch (e) { setError(errorText(e)); } finally { setBusy(null); }
  };

  return (
    <div className="max-w-3xl space-y-4">
      {error && <ErrorLine text={error} />}
      {done && <Notice tone="ok">{done}</Notice>}
      <Card className="space-y-5 p-6">
        <div><h3 className="font-heading text-lg font-extrabold">Installed server</h3><p className="mt-1 font-mono text-sm text-muted-foreground">{running ? `v${running.version} · ${running.revision} · build ${running.buildId}` : "Loading…"}</p></div>
        <div className="border-t pt-5">
          <div className="flex flex-wrap items-end gap-2">
            <label className="grid gap-1.5 text-sm"><span className="font-medium">Release</span><select className="h-9 rounded-md border border-input bg-background px-3" value={channel} onChange={(e) => { setChannel(e.target.value as typeof channel); setPreview(null); }}><option value="stable">Latest stable</option><option value="prerelease">Latest prerelease</option><option value="tag">Exact tag</option></select></label>
            {channel === "tag" && <label className="grid gap-1.5 text-sm"><span className="font-medium">Tag</span><input className="h-9 w-40 rounded-md border border-input bg-background px-3 font-mono" placeholder="v0.2.0" value={tag} onChange={(e) => { setTag(e.target.value); setPreview(null); }} /></label>}
            <Button disabled={!!busy || (channel === "tag" && !tag.trim())} onClick={() => void checkRelease()}><Download className="size-4" /> Check release</Button>
          </div>
          {preview && <div className="mt-4 flex flex-wrap items-center gap-4 rounded-lg border bg-muted/50 p-4"><div className="mr-auto"><p className="font-medium">{preview.name}</p><p className="mt-1 font-mono text-xs text-muted-foreground">v{preview.version} · {preview.revision} · build {preview.buildId}</p></div><Button variant="primary" disabled={!!busy} onClick={() => void installRelease()}>Install and restart</Button></div>}
        </div>
      </Card>
      <details className="rounded-lg border bg-card"><summary className="cursor-pointer px-4 py-3 text-sm font-medium">Manual binary</summary><div className="space-y-3 border-t p-4"><p className="text-sm text-destructive">Manual binaries are not signature-verified. Use only a binary you built or received from a trusted source.</p><div className="flex gap-2"><input className="h-9 w-44 rounded-md border border-input bg-background px-3 text-sm" placeholder="Version label" value={version} onChange={(e) => setVersion(e.target.value)} /><Button disabled={!!busy || !version.trim()} onClick={() => void manual()}><Upload className="size-4" /> Choose binary</Button></div></div></details>
      {busy && <p className="text-sm text-muted-foreground">{busy}</p>}
    </div>
  );
}
