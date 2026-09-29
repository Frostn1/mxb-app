import { useCallback, useEffect, useRef, useState } from "react";
import { errorText, legacyConfig, legacyConfigSave, serverTracks, type LegacyStatus, type ServerView } from "@/lib/api";
import { Button, Card, ErrorLine, Field, Input } from "./ui";

const cache = new Map<string, { status: LegacyStatus; tracks: string[] }>();

export function LegacySettings({ server }: { server: ServerView }) {
  const saved = cache.get(server.id);
  const [name, setName] = useState(saved?.status.server.name ?? "");
  const [track, setTrack] = useState(saved?.status.server.track ?? "");
  const [maxClients, setMaxClients] = useState(Number(saved?.status.server.maxClients ?? 20));
  const [tracks, setTracks] = useState(saved?.tracks ?? []);
  const [baseline, setBaseline] = useState(() => saved ? JSON.stringify({ name: saved.status.server.name ?? "", track: saved.status.server.track ?? "", maxClients: Number(saved.status.server.maxClients ?? 20) }) : "");
  const loadedInitially = useRef(saved != null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  const load = useCallback(async () => {
    setError(null);
    const [status, trackState] = await Promise.all([legacyConfig(server.id), serverTracks(server.id)]);
    const values = {
      name: status.server.name ?? "",
      track: status.server.track ?? "",
      maxClients: Number(status.server.maxClients ?? 20),
    };
    cache.set(server.id, { status, tracks: trackState.installed });
    setName(values.name); setTrack(values.track); setMaxClients(values.maxClients);
    setTracks(trackState.installed);
    setBaseline(JSON.stringify(values));
  }, [server.id]);

  useEffect(() => { if (!loadedInitially.current) void load().catch((e) => setError(errorText(e))); }, [load]);
  useEffect(() => { if (!done) return; const timer = window.setTimeout(() => setDone(false), 3000); return () => clearTimeout(timer); }, [done]);

  const changed = baseline !== "" && baseline !== JSON.stringify({ name, track, maxClients });
  const save = async () => {
    if (!changed || !window.confirm(`Save these settings and restart ${server.name}?`)) return;
    setBusy(true); setError(null); setDone(false);
    try { await legacyConfigSave(server.id, name, track, maxClients); await load(); setDone(true); }
    catch (e) { setError(errorText(e)); }
    finally { setBusy(false); }
  };

  return (
    <div className="max-w-3xl space-y-4">
      {error && <ErrorLine text={error} />}
      {done && <div role="status" className="fixed right-5 top-12 z-50 rounded-lg border bg-card px-4 py-3 text-sm font-medium shadow-lg">Settings saved.</div>}
      <Card className="space-y-5 p-6">
        <Field label="Server name"><Input value={name} onChange={(e) => setName(e.target.value)} /></Field>
        <Field label="Track">
          <select className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm" value={track} onChange={(e) => setTrack(e.target.value)}>
            {!tracks.includes(track) && track && <option value={track}>{track}</option>}
            {tracks.map((item) => <option key={item} value={item}>{item}</option>)}
          </select>
        </Field>
        <Field label="Maximum riders"><Input type="number" min={1} max={50} value={maxClients} onChange={(e) => setMaxClients(Number(e.target.value))} /></Field>
      </Card>
      <div className="flex justify-end gap-2">
        <Button disabled={!changed || busy} onClick={() => void load()}>Reset</Button>
        <Button variant="primary" disabled={!changed || busy} onClick={() => void save()}>{busy ? "Saving…" : "Save"}</Button>
      </div>
    </div>
  );
}
