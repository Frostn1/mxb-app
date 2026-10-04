import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowDown, ArrowUp, GripVertical, Plus, Search, Trash2, Upload } from "lucide-react";
import { errorText, inspectTrackUpload, serverRestartService, serverSession, serverSetRotation, serverSetTrack, serverTracks, type ServerView, type TrackState } from "@/lib/api";
import { addBlockedReason, blockedInQueue, describeRotationSave, isProtected, PROTECTED_REASON, playNextQueue, randomTrack, stateAfterSwitch } from "@/lib/rotation";
import { runAction } from "@/lib/actions";
import { byteSize } from "@/lib/format";
import { initUploads, onUploadSettled, startTrackUpload, useUploads } from "@/lib/uploads";
import { Button, Card, ErrorLine, Notice } from "./ui";
import { ReloadBadge } from "./ConfigTab";
import { UploadRow } from "./UploadsIndicator";

const trackCache = new Map<string, TrackState>();

export function TracksTab({ server }: { server: ServerView }) {
  const cached = trackCache.get(server.id) ?? null;
  const [state, setState] = useState<TrackState | null>(cached);
  const [queue, setQueue] = useState<string[]>(cached ? [...(cached.current ? [cached.current] : []), ...cached.rotation] : []);
  const [query, setQuery] = useState("");
  const [dragged, setDragged] = useState<number | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  const load = useCallback(async (quiet = false) => {
    if (!quiet) setError(null);
    try {
      const tracks = await serverTracks(server.id);
      trackCache.set(server.id, tracks);
      setState(tracks);
      setQueue([...(tracks.current ? [tracks.current] : []), ...tracks.rotation]);
    } catch (e) { if (!quiet) setError(errorText(e)); }
  }, [server.id]);
  useEffect(() => { void load(); }, [load]);
  // Uploads belong to the app, not to this tab: whatever is running for this server shows up
  // again here, and a finish while this tab is open refreshes the library.
  const serverUploads = useUploads().filter((u) => u.serverId === server.id);
  useEffect(() => {
    void initUploads();
    return onUploadSettled((u) => {
      if (u.serverId !== server.id || u.status !== "done") return;
      setDone(`${u.fileName} installed. Add it to the rotation; no restart needed.`);
      void load();
    });
  }, [server.id, load]);
  useEffect(() => {
    if (!done) return;
    const timer = window.setTimeout(() => setDone(null), 3000);
    return () => window.clearTimeout(timer);
  }, [done]);

  const available = useMemo(() => Array.from(new Set([...(state?.library ?? []), ...(state?.installed ?? [])])).sort(), [state]);
  const installed = useMemo(() => available.filter((track) => track.toLowerCase().includes(query.toLowerCase())), [available, query]);
  const savedQueue = [...(state?.current ? [state.current] : []), ...(state?.rotation ?? [])];
  const changed = !!state && JSON.stringify(queue) !== JSON.stringify(savedQueue);
  const move = (from: number, to: number) => {
    if (to < 0 || to >= queue.length || from === to) return;
    setQueue((old) => { const next = [...old]; const [item] = next.splice(from, 1); next.splice(to, 0, item); return next; });
  };
  /** A server too old to change tracks while it runs keeps the saved order for a restart: say so,
   *  and restart only when the user agrees (riders are disconnected). */
  const restartForRotation = async (message: string) => {
    if (!window.confirm(`${message}\n\nRestart ${server.name} now? Connected riders will be disconnected.`)) { setDone(message); return; }
    setBusy("Restarting…");
    await serverRestartService(server.id);
    setDone("Restarted with the new rotation.");
  };
  const protectedTracks = state?.protected;
  const blockedNow = blockedInQueue(queue, protectedTracks);
  const addTrack = (track: string) => {
    const why = addBlockedReason(track, protectedTracks);
    if (why) { setError(why); return; }
    setQueue((q) => [...q, track]);
  };
  const save = async () => {
    if (!queue.length || !state) return;
    if (blockedNow.length) { setError(`${blockedNow.join(", ")}: ${PROTECTED_REASON}. Remove it from the rotation to save.`); return; }
    setBusy("Saving rotation…"); setError(null); setDone(null);
    const before = state;
    // The saved rotation is on screen at once; it is undone if the server refuses it.
    const result = await runAction({
      name: "save_rotation",
      optimistic: () => { setState(stateAfterSwitch(before, queue)); return () => setState(before); },
      run: () => serverSetRotation(server.id, queue),
    });
    if (!result.ok) { setError(result.error); setBusy(null); return; }
    try {
      const said = describeRotationSave(result.value);
      if (said.needsRestart) { setState(before); await restartForRotation(said.message); await load(); } else { trackCache.set(server.id, stateAfterSwitch(before, queue)); setDone(said.message); void load(true); }
    } catch (e) { setError(errorText(e)); } finally { setBusy(null); }
  };  const upload = async () => {
    let path = await open({ multiple: false, directory: false, filters: [{ name: "MXB track package", extensions: ["pkz"] }] });
    if (!path) return;
    setBusy("Checking track…"); setError(null); setDone(null);
    let check;
    try { check = await inspectTrackUpload(path); }
    catch (e) { setBusy(null); setError(errorText(e)); return; }
    if (check.protected) {
      const alt = check.alternative;
      const altName = alt?.split(/[\\/]/).pop();
      if (!alt || !window.confirm(`${check.detail}\n\nUse ${altName} (the server version of this track) instead?`)) {
        setBusy(null);
        if (!alt) setError(`${check.uploadName}: ${PROTECTED_REASON}. It was not uploaded, and no server version was found next to it or in Downloads.`);
        return;
      }
      try { check = await inspectTrackUpload(alt); path = alt; }
      catch (e) { setBusy(null); setError(errorText(e)); return; }
      if (check.protected) { setBusy(null); setError(`${check.uploadName}: ${PROTECTED_REASON}.`); return; }
    }
    const size = byteSize(check.bytes);
    const question = check.serverTrack
      ? `Upload ${check.uploadName} (${size}) to this machine and add it to ${server.name}?`
      : `${check.detail}

It will be stored as ${check.uploadName} (${size}). It may be a full client track and use unnecessary server storage. Upload it anyway?`;
    if (!window.confirm(question)) { setBusy(null); return; }
    // Started in the backend: it keeps going when this tab is left, and the header shows it.
    try { await startTrackUpload(server.id, path ?? ""); }
    catch (e) { setError(errorText(e)); } finally { setBusy(null); }
  };
  const switchTrack = async (mode: "next" | "random") => {
    if (!state?.current || state.rotation.length === 0) return;
    const runnable = state.rotation.filter((track) => !isProtected(track, state.protected));
    const selected = mode === "next" ? state.rotation[0] : randomTrack(runnable);
    if (!selected) { setError(`${PROTECTED_REASON}: nothing in the rotation can be played.`); return; }
    const why = addBlockedReason(selected, state.protected);
    if (why) { setError(why); return; }
    const nextQueue = playNextQueue(state.current, state.rotation, selected);
    const label = mode === "next" ? `Play ${selected} next?` : `Switch to the randomly selected track ${selected}?`;
    if (!window.confirm(`${label} The event ends now and riders reload the new track. ${server.name} does not restart.`)) return;
    // Show the result at once; the server confirms (or this is rolled back) when it answers.
    const before = { state, queue };
    setBusy(`Loading ${selected}…`); setError(null); setDone(null);
    setState(stateAfterSwitch(state, nextQueue)); setQueue(nextQueue);
    const rollBack = () => { setState(before.state); setQueue(before.queue); };
    try {
      const said = describeRotationSave(await serverSetRotation(server.id, nextQueue));
      if (said.needsRestart) { rollBack(); await restartForRotation(said.message); await load(); }
      else {
        await serverSession(server.id, "rotate");
        trackCache.set(server.id, stateAfterSwitch(state, nextQueue));
        setDone(`${selected} is now playing.`);
        // The new state is already on screen: confirm it with the server without making the user wait.
        void load(true);
      }
    } catch (e) { rollBack(); setError(errorText(e)); void load(true); } finally { setBusy(null); }
  };
  const selectLegacyTrack = async (track: string) => {
    if (track === state?.current || !window.confirm(`Switch ${server.name} to ${track}? The official server will restart.`)) return;
    setBusy(`Switching to ${track}…`); setError(null); setDone(null);
    try { await serverSetTrack(server.id, track); setDone(`${track} selected.`); await load(); }
    catch (e) { setError(errorText(e)); } finally { setBusy(null); }
  };

  if (server.kind === "legacy") return (
    <div className="max-w-3xl space-y-4">
      {error && <ErrorLine text={error} />}
      {done && <div role="status" className="fixed right-5 top-12 z-50 rounded-lg border bg-card px-4 py-3 text-sm font-medium shadow-lg">{done}</div>}
      <Card className="flex min-h-[24rem] flex-col gap-4">
        <div><h3 className="font-heading text-lg font-extrabold">Installed tracks</h3><p className="text-xs text-muted-foreground">From the official server&apos;s mods folder</p></div>
        <label className="relative"><Search className="absolute left-3 top-2.5 size-4 text-muted-foreground" /><input className="h-9 w-full rounded-md border border-input bg-background pl-9 pr-3 text-sm" placeholder="Search tracks" value={query} onChange={(e) => setQuery(e.target.value)} /></label>
        <div className="divide-y overflow-auto">
          {state === null && <p className="py-3 text-sm text-muted-foreground">Loading…</p>}
          {state && installed.length === 0 && <p className="py-3 text-sm text-muted-foreground">No matching tracks.</p>}
          {installed.map((item) => <div key={item} className="flex items-center gap-3 py-3"><span className="min-w-0 flex-1 truncate text-sm font-medium">{item}</span>{item === state?.current ? <span className="text-xs font-medium text-primary">Current</span> : <Button size="sm" disabled={!!busy} onClick={() => void selectLegacyTrack(item)}>Select</Button>}</div>)}
        </div>
      </Card>
      {busy && <p className="text-sm text-muted-foreground">{busy}</p>}
    </div>
  );

  if (server.local) return <Notice>Connect this server over SSH to manage tracks and versions.</Notice>;
  return (
    <div className="flex flex-col gap-5">
      {error && <ErrorLine text={error} />}
      {done && <div role="status" className="fixed right-5 top-12 z-50 rounded-lg border bg-card px-4 py-3 text-sm font-medium shadow-lg">{done}</div>}
      <div className="grid gap-5 lg:grid-cols-[minmax(16rem,0.75fr)_minmax(24rem,1.25fr)]">
        <Card className="flex min-h-[30rem] flex-col gap-4 overflow-hidden">
          <div className="flex items-center justify-between gap-3"><div><h3 className="font-heading text-lg font-extrabold">Track library</h3><p className="text-xs text-muted-foreground">The .pkz files in the server&apos;s track folder</p></div><Button size="sm" onClick={() => void upload()} disabled={!!busy}><Upload className="size-3.5" /> Upload</Button></div>
          {serverUploads.length > 0 && <div className="divide-y rounded-lg border px-3">{serverUploads.map((u) => <UploadRow key={u.id} upload={u} showServer={false} />)}</div>}
          <label className="relative"><Search className="absolute left-3 top-2.5 size-4 text-muted-foreground" /><input className="h-9 w-full rounded-md border border-input bg-background pl-9 pr-3 text-sm outline-none focus:border-ring" placeholder="Search tracks" value={query} onChange={(e) => setQuery(e.target.value)} /></label>
          <div className="flex min-h-0 flex-1 flex-col divide-y overflow-auto">
            {state === null && <p className="py-3 text-sm text-muted-foreground">Loading…</p>}
            {state && installed.length === 0 && <p className="py-3 text-sm text-muted-foreground">No matching tracks.</p>}
            {installed.map((track) => {
              return <div key={track} className="flex items-center gap-2 py-2.5"><span className="min-w-0 flex-1 truncate text-sm font-medium">{track}</span>{track === state?.current && <span className="text-xs font-medium text-primary">Active</span>}{isProtected(track, protectedTracks) && <span className="text-xs text-destructive">{PROTECTED_REASON}</span>}<button type="button" title={isProtected(track, protectedTracks) ? PROTECTED_REASON : "Add to rotation"} disabled={isProtected(track, protectedTracks)} onClick={() => addTrack(track)} className="grid size-8 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30"><Plus className="size-4" /></button></div>;
            })}
          </div>
        </Card>

        <Card className="flex min-h-[26rem] flex-col gap-4">
          <div className="flex flex-wrap items-center justify-between gap-3"><div><h3 className="font-heading text-lg font-extrabold">Track rotation</h3><p className="text-xs text-muted-foreground">Drag to reorder. Changes save without a restart<ReloadBadge kind="next_event" /></p></div><div className="flex flex-wrap justify-end gap-2"><Button size="sm" disabled={!!busy || changed || !state?.rotation.length} title={changed ? "Save or reset your rotation changes first" : undefined} onClick={() => void switchTrack("next")}>Next track</Button><Button size="sm" disabled={!!busy || changed || !state?.rotation.length} title={changed ? "Save or reset your rotation changes first" : undefined} onClick={() => void switchTrack("random")}>Random track</Button><Button size="sm" disabled={!changed || !!busy} onClick={() => setQueue(savedQueue)}>Reset</Button><Button size="sm" variant="primary" disabled={!changed || !queue.length || !!busy || blockedNow.length > 0} title={blockedNow.length ? `${blockedNow.join(", ")}: ${PROTECTED_REASON}` : undefined} onClick={() => void save()}>Save rotation</Button></div></div>
          <div
            className="-mx-4 -mb-4 min-h-0 flex-1 overflow-auto border-t p-8"
            style={{ backgroundImage: "radial-gradient(circle, color-mix(in srgb, var(--muted-foreground) 25%, transparent) 1px, transparent 1px)", backgroundSize: "20px 20px" }}
          >
            {queue.length === 0 && <div className="grid h-full min-h-64 place-items-center"><button type="button" className="rounded-xl border border-dashed bg-card px-8 py-6 text-sm text-muted-foreground" onClick={() => document.querySelector<HTMLInputElement>('input[placeholder="Search tracks"]')?.focus()}>Choose a track to start the flow</button></div>}
            <div className="flex min-w-max items-center py-12">
              {queue.map((track, index) => (
                <div key={`${track}-${index}`} className="flex items-center">
                  {index > 0 && <div className="relative h-0.5 w-14 bg-border"><span className="absolute -right-1 -top-[3px] size-2 rotate-45 border-r-2 border-t-2 border-border" /></div>}
                  <div
                    draggable
                    onDragStart={() => setDragged(index)}
                    onDragOver={(e) => e.preventDefault()}
                    onDrop={() => { if (dragged != null) move(dragged, index); setDragged(null); }}
                    className={`group relative w-52 rounded-xl border bg-card shadow-sm transition hover:border-primary/60 hover:shadow-md ${index === 0 ? "border-primary/60 ring-2 ring-primary/10" : ""} ${dragged === index ? "opacity-40" : ""}`}
                  >
                    <div className="flex items-center gap-2 border-b px-3 py-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                      <GripVertical className="size-3.5 cursor-grab" />
                      <span>{index === 0 ? "Start here" : index === 1 ? "Up next" : `Step ${index + 1}`}</span>
                      <span className={`ml-auto size-2 rounded-full ${index === 0 ? "bg-primary" : "bg-muted-foreground/35"}`} />
                    </div>
                    <div className="px-4 py-4">
                      <div className="truncate font-heading text-sm font-bold" title={track}>{track}</div>
                      <div className="mt-1 text-xs text-muted-foreground">MX Bikes track</div>
                    </div>
                    <div className="flex items-center justify-end gap-0.5 border-t px-2 py-1.5 opacity-60 transition group-hover:opacity-100 group-focus-within:opacity-100">
                      <button aria-label={`Move ${track} earlier`} title="Move earlier" disabled={index === 0} onClick={() => move(index, index - 1)} className="grid size-7 place-items-center rounded text-muted-foreground hover:bg-accent disabled:opacity-20"><ArrowUp className="size-3.5 -rotate-90" /></button>
                      <button aria-label={`Move ${track} later`} title="Move later" disabled={index === queue.length - 1} onClick={() => move(index, index + 1)} className="grid size-7 place-items-center rounded text-muted-foreground hover:bg-accent disabled:opacity-20"><ArrowDown className="size-3.5 -rotate-90" /></button>
                      <button aria-label={`Remove ${track}`} title="Remove" onClick={() => setQueue((q) => q.filter((_, i) => i !== index))} className="grid size-7 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-destructive"><Trash2 className="size-3.5" /></button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </Card>
      </div>
      {busy && <p className="text-sm text-muted-foreground">{busy}</p>}
    </div>
  );
}
