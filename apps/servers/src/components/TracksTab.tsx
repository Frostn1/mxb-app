import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowDown, ArrowUp, GripVertical, Plus, Search, Trash2, Upload } from "lucide-react";
import { errorText, serverSetRotation, serverStatus, serverTracks, serverUpdateGithub, serverUpload, type ServerView, type TrackState } from "@/lib/api";
import { Button, Card, ErrorLine, Notice } from "./ui";

export function TracksTab({ server }: { server: ServerView }) {
  const [state, setState] = useState<TrackState | null>(null);
  const [queue, setQueue] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [dragged, setDragged] = useState<number | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const [version, setVersion] = useState("");
  const [running, setRunning] = useState<{ version: string; revision: string } | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [tracks, status] = await Promise.all([serverTracks(server.id), serverStatus(server.id)]);
      setState(tracks);
      setQueue([...(tracks.current ? [tracks.current] : []), ...tracks.rotation]);
      if (status.status) setRunning({ version: status.status.version, revision: status.status.revision });
    } catch (e) { setError(errorText(e)); }
  }, [server.id]);
  useEffect(() => { void load(); }, [load]);

  const installed = useMemo(() => (state?.installed ?? []).filter((track) => track.toLowerCase().includes(query.toLowerCase())), [query, state]);
  const savedQueue = [...(state?.current ? [state.current] : []), ...(state?.rotation ?? [])];
  const changed = !!state && JSON.stringify(queue) !== JSON.stringify(savedQueue);
  const move = (from: number, to: number) => {
    if (to < 0 || to >= queue.length || from === to) return;
    setQueue((old) => { const next = [...old]; const [item] = next.splice(from, 1); next.splice(to, 0, item); return next; });
  };
  const save = async () => {
    if (!queue.length || !window.confirm(`Save this track order? ${server.name} will restart.`)) return;
    setBusy("Saving rotation…"); setError(null); setDone(null);
    try { await serverSetRotation(server.id, queue); setDone("Track rotation saved."); await load(); }
    catch (e) { setError(errorText(e)); } finally { setBusy(null); }
  };
  const upload = async (kind: "track" | "version") => {
    if (kind === "version" && !version.trim()) { setError("Enter the version first."); return; }
    const path = await open({ multiple: false, directory: false, filters: kind === "track" ? [{ name: "MXB track package", extensions: ["pkz"] }] : undefined });
    if (!path) return;
    setBusy(kind === "track" ? "Uploading track…" : "Updating server…"); setError(null); setDone(null);
    try { await serverUpload(server.id, kind, path, kind === "version" ? version.trim() : undefined); setDone(kind === "track" ? "Track installed." : `Updated to ${version.trim()}.`); await load(); }
    catch (e) { setError(errorText(e)); } finally { setBusy(null); }
  };
  const updateFromGithub = async () => {
    if (!window.confirm(`Update ${server.name} to the latest mxbserver release?`)) return;
    setBusy("Downloading the latest release…"); setError(null); setDone(null);
    try { await serverUpdateGithub(server.id); setDone("Server updated from GitHub."); await load(); }
    catch (e) { setError(errorText(e)); } finally { setBusy(null); }
  };

  if (server.local) return <Notice>Connect this server over SSH to manage tracks and versions.</Notice>;
  return (
    <div className="flex flex-col gap-5">
      {error && <ErrorLine text={error} />}
      {done && <Notice tone="ok">{done}</Notice>}
      <div className="grid gap-5 lg:grid-cols-[minmax(16rem,0.75fr)_minmax(24rem,1.25fr)]">
        <Card className="flex min-h-[26rem] flex-col gap-4">
          <div className="flex items-center justify-between gap-3"><h3 className="font-heading text-lg font-extrabold">Installed tracks</h3><Button size="sm" onClick={() => void upload("track")} disabled={!!busy}><Upload className="size-3.5" /> Upload</Button></div>
          <label className="relative"><Search className="absolute left-3 top-2.5 size-4 text-muted-foreground" /><input className="h-9 w-full rounded-md border border-input bg-background pl-9 pr-3 text-sm outline-none focus:border-ring" placeholder="Search tracks" value={query} onChange={(e) => setQuery(e.target.value)} /></label>
          <div className="flex min-h-0 flex-1 flex-col divide-y overflow-auto">
            {state === null && <p className="py-3 text-sm text-muted-foreground">Loading…</p>}
            {state && installed.length === 0 && <p className="py-3 text-sm text-muted-foreground">No matching tracks.</p>}
            {installed.map((track) => <div key={track} className="flex items-center gap-3 py-2.5"><span className="min-w-0 flex-1 truncate text-sm font-medium">{track}</span><button type="button" title="Add to rotation" onClick={() => setQueue((q) => [...q, track])} className="grid size-8 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"><Plus className="size-4" /></button></div>)}
          </div>
        </Card>

        <Card className="flex min-h-[26rem] flex-col gap-4">
          <div className="flex flex-wrap items-center justify-between gap-3"><div><h3 className="font-heading text-lg font-extrabold">Track rotation</h3><p className="text-xs text-muted-foreground">Drag to reorder.</p></div><div className="flex gap-2"><Button size="sm" disabled={!changed || !!busy} onClick={() => setQueue(savedQueue)}>Reset</Button><Button size="sm" variant="primary" disabled={!changed || !queue.length || !!busy} onClick={() => void save()}>Save rotation</Button></div></div>
          <div className="flex flex-col gap-2">
            {queue.length === 0 && <div className="rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">Add tracks from the library.</div>}
            {queue.map((track, index) => (
              <div key={`${track}-${index}`} draggable onDragStart={() => setDragged(index)} onDragOver={(e) => e.preventDefault()} onDrop={() => { if (dragged != null) move(dragged, index); setDragged(null); }} className={`flex items-center gap-3 rounded-lg border bg-background px-3 py-3 ${dragged === index ? "opacity-50" : ""}`}>
                <GripVertical className="size-4 cursor-grab text-muted-foreground" /><span className="grid size-7 shrink-0 place-items-center rounded-full bg-secondary font-mono text-xs">{index + 1}</span>
                <div className="min-w-0 flex-1"><div className="truncate text-sm font-semibold">{track}</div><div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{index === 0 ? "Current" : index === 1 ? "Up next" : "Queued"}</div></div>
                <button title="Move up" disabled={index === 0} onClick={() => move(index, index - 1)} className="grid size-7 place-items-center rounded text-muted-foreground hover:bg-accent disabled:opacity-25"><ArrowUp className="size-3.5" /></button>
                <button title="Move down" disabled={index === queue.length - 1} onClick={() => move(index, index + 1)} className="grid size-7 place-items-center rounded text-muted-foreground hover:bg-accent disabled:opacity-25"><ArrowDown className="size-3.5" /></button>
                <button title="Remove" onClick={() => setQueue((q) => q.filter((_, i) => i !== index))} className="grid size-7 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-destructive"><Trash2 className="size-3.5" /></button>
              </div>
            ))}
          </div>
        </Card>
      </div>
      <Card className="flex flex-wrap items-center gap-4">
        <div className="mr-auto"><h3 className="font-heading text-base font-extrabold">Server version</h3><p className="font-mono text-xs text-muted-foreground">{running ? `v${running.version} · ${running.revision}` : "Loading…"}</p></div>
        <Button variant="primary" disabled={!!busy} onClick={() => void updateFromGithub()}>Update from GitHub</Button>
        <details className="relative"><summary className="cursor-pointer text-sm text-muted-foreground">Manual upload</summary><div className="absolute bottom-8 right-0 z-10 flex w-max gap-2 rounded-lg border bg-card p-3 shadow-lg"><input className="h-9 w-40 rounded-md border border-input bg-background px-3 text-sm" placeholder="Version" value={version} onChange={(e) => setVersion(e.target.value)} /><Button disabled={!!busy || !version.trim()} onClick={() => void upload("version")}><Upload className="size-4" /> Upload binary</Button></div></details>
      </Card>
      {busy && <p className="text-sm text-muted-foreground">{busy}</p>}
    </div>
  );
}
