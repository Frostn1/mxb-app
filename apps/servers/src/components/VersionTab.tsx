import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Upload } from "lucide-react";
import { errorText, serverStatus, serverUpdateGithub, serverUpload, type ServerView } from "@/lib/api";
import { Button, Card, ErrorLine, Notice } from "./ui";

const versionCache = new Map<string, { version: string; revision: string }>();

export function VersionTab({ server }: { server: ServerView }) {
  const [running, setRunning] = useState<{ version: string; revision: string } | null>(versionCache.get(server.id) ?? null);
  const [version, setVersion] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const load = useCallback(async () => {
    const report = await serverStatus(server.id);
    if (report.status) {
      const value = { version: report.status.version, revision: report.status.revision };
      versionCache.set(server.id, value);
      setRunning(value);
    }
  }, [server.id]);
  useEffect(() => { void load().catch((e) => setError(errorText(e))); }, [load]);

  const github = async () => {
    if (!window.confirm(`Update ${server.name} to the latest official release?`)) return;
    setBusy("Downloading latest release…"); setError(null); setDone(null);
    try { await serverUpdateGithub(server.id); setDone("Server updated."); await load(); }
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
      <Card className="flex flex-wrap items-center gap-5 p-6">
        <div className="mr-auto"><h3 className="font-heading text-lg font-extrabold">mxbserver</h3><p className="mt-1 font-mono text-sm text-muted-foreground">{running ? `v${running.version} · ${running.revision}` : "Loading…"}</p></div>
        <Button variant="primary" disabled={!!busy} onClick={() => void github()}>Update from GitHub</Button>
      </Card>
      <details className="rounded-lg border bg-card"><summary className="cursor-pointer px-4 py-3 text-sm font-medium">Manual binary</summary><div className="flex gap-2 border-t p-4"><input className="h-9 w-44 rounded-md border border-input bg-background px-3 text-sm" placeholder="Version" value={version} onChange={(e) => setVersion(e.target.value)} /><Button disabled={!!busy || !version.trim()} onClick={() => void manual()}><Upload className="size-4" /> Choose binary</Button></div></details>
      {busy && <p className="text-sm text-muted-foreground">{busy}</p>}
    </div>
  );
}
