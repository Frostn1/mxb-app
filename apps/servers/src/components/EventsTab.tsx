import { useMemo, useState } from "react";
import { ChevronRight, Pause, Play, Trash2 } from "lucide-react";
import type { ServerView } from "@/lib/api";
import { countKinds, EVENT_KINDS, filterEvents, MAX_EVENTS, type EventKind, type FieldValue, type ServerEvent } from "@/lib/events";
import type { EventFeed } from "@/lib/useEventFeed";
import { Button, ErrorLine, Input, Notice } from "./ui";

/** A colour per kind, on the chip dot and the row marker. */
const KIND_COLOR: Record<EventKind, string> = {
  lap: "var(--success)",
  split: "var(--primary)",
  collision: "var(--destructive)",
  cut: "#d97706",
  crash: "#dc2626",
  penalty: "#c026d3",
  race: "#0891b2",
  join: "#16a34a",
  leave: "#64748b",
  session: "#7c3aed",
  refused: "#b45309",
  info: "var(--muted-foreground)",
};

const KIND_LABEL = Object.fromEntries(EVENT_KINDS.map(({ kind, label }) => [kind, label])) as Record<EventKind, string>;

/** Everything but the chatty kinds, until changed. */
const DEFAULT_KINDS: EventKind[] = EVENT_KINDS.map(({ kind }) => kind).filter((kind) => kind !== "info" && kind !== "refused");

const time = (at: number) => new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });

const show = (value: FieldValue) => (value == null ? "—" : typeof value === "boolean" ? (value ? "Yes" : "No") : String(value));

const SOURCE: Record<ServerEvent["source"], string> = {
  log: "Server log",
  timing: "Live timing (/timing)",
  riders: "Rider list (/v1/riders)",
};

export function EventsTab({ server, feed }: { server: ServerView; feed: EventFeed }) {
  const [kinds, setKinds] = useState<Set<EventKind>>(() => new Set(DEFAULT_KINDS));
  const [query, setQuery] = useState("");
  const [paused, setPaused] = useState<ServerEvent[] | null>(null);
  const [open, setOpen] = useState<string | null>(null);

  const source = paused ?? feed.events;
  const counts = useMemo(() => countKinds(source), [source]);
  const shown = useMemo(() => filterEvents(source, kinds, query), [source, kinds, query]);
  const waiting = paused ? feed.events.length - paused.length : 0;

  if (server.kind === "legacy") {
    return <Notice>Events come from mxbserver&apos;s log and live timing. The official dedicated server doesn&apos;t report them.</Notice>;
  }

  const toggle = (kind: EventKind) =>
    setKinds((current) => {
      const next = new Set(current);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });

  return (
    <div className="flex h-full flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search riders" aria-label="Search riders" className="w-56" />
        <Button onClick={() => setPaused(paused ? null : feed.events)} aria-pressed={!!paused}>
          {paused ? <Play className="size-4" /> : <Pause className="size-4" />}
          {paused ? `Resume${waiting > 0 ? ` (${waiting} new)` : ""}` : "Pause"}
        </Button>
        <Button
          variant="ghost"
          onClick={() => {
            feed.clear();
            setPaused(paused ? [] : null);
            setOpen(null);
          }}
        >
          <Trash2 className="size-4" /> Clear
        </Button>
        <span className="ml-auto text-xs text-muted-foreground">
          {shown.length} of {source.length} events{source.length >= MAX_EVENTS ? ` (newest ${MAX_EVENTS} kept)` : ""}
        </span>
      </div>

      <div className="flex flex-wrap items-center gap-1.5" role="group" aria-label="Event types">
        {EVENT_KINDS.map(({ kind, label }) => {
          const on = kinds.has(kind);
          return (
            <button
              key={kind}
              type="button"
              aria-pressed={on}
              onClick={() => toggle(kind)}
              className={`inline-flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium transition ${
                on ? "border-primary/40 bg-accent text-accent-foreground" : "text-muted-foreground hover:bg-accent/50 hover:text-foreground"
              }`}
            >
              <span className="size-2 rounded-full" style={{ background: KIND_COLOR[kind], opacity: on ? 1 : 0.45 }} aria-hidden />
              {label}
              <span className="font-mono tabular-nums opacity-70">{counts[kind]}</span>
            </button>
          );
        })}
        <button type="button" onClick={() => setKinds(new Set(EVENT_KINDS.map((k) => k.kind)))} className="px-1.5 text-xs text-muted-foreground hover:text-foreground">
          All
        </button>
        <button type="button" onClick={() => setKinds(new Set())} className="px-1.5 text-xs text-muted-foreground hover:text-foreground">
          None
        </button>
      </div>

      {feed.errors.log && <ErrorLine text={`Log: ${feed.errors.log}`} />}
      {feed.errors.timing && <ErrorLine text={`Live timing: ${feed.errors.timing}`} />}

      <div className="min-h-0 flex-1 overflow-auto rounded-lg border bg-card">
        {shown.length === 0 ? (
          <p className="p-4 text-sm text-muted-foreground">
            {source.length === 0 ? "Waiting for events. Laps and time checks appear as riders cross the timing gates." : "No events match these filters."}
          </p>
        ) : (
          <ul className="divide-y">
            {shown.map((event) => (
              <EventRow key={event.id} event={event} open={open === event.id} onToggle={() => setOpen(open === event.id ? null : event.id)} />
            ))}
          </ul>
        )}
      </div>
      <p className="text-xs text-muted-foreground">
        From {server.local ? server.logPath : "journalctl -u mxbserver"}, live timing{feed.named ? " and the rider list" : ""}. Times are when this app saw each event.
      </p>
    </div>
  );
}

function EventRow({ event, open, onToggle }: { event: ServerEvent; open: boolean; onToggle: () => void }) {
  const fields = Object.entries(event.fields);
  return (
    <li>
      <button type="button" onClick={onToggle} aria-expanded={open} className="flex w-full items-center gap-3 px-3 py-2 text-left text-sm hover:bg-accent/40">
        <ChevronRight className={`size-3.5 shrink-0 text-muted-foreground transition ${open ? "rotate-90" : ""}`} aria-hidden />
        <span className="w-16 shrink-0 font-mono text-xs text-muted-foreground tabular-nums" title={event.backlog ? "Already in the log when the tab opened" : undefined}>
          {event.backlog ? "earlier" : time(event.at)}
        </span>
        <span className="inline-flex w-24 shrink-0 items-center gap-1.5 text-xs font-medium" style={{ color: KIND_COLOR[event.kind] }}>
          <span className="size-2 rounded-full" style={{ background: KIND_COLOR[event.kind] }} aria-hidden />
          {KIND_LABEL[event.kind]}
        </span>
        <span className="min-w-0 flex-1 truncate">{event.summary}</span>
      </button>
      {open && (
        <div className="flex flex-col gap-3 bg-muted/60 px-10 py-3 text-sm">
          {fields.length > 0 && (
            <dl className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-x-6 gap-y-2">
              {fields.map(([key, value]) => (
                <div key={key} className="flex flex-col gap-0.5">
                  <dt className="text-xs text-muted-foreground">{key}</dt>
                  <dd className="font-mono text-sm tabular-nums break-words">{show(value)}</dd>
                </div>
              ))}
            </dl>
          )}
          <div className="flex flex-wrap gap-x-6 gap-y-1 text-xs text-muted-foreground">
            <span>Source: {SOURCE[event.source]}</span>
            <span>Seen: {event.backlog ? "before the tab opened" : new Date(event.at).toLocaleString()}</span>
          </div>
          {event.raw && <pre className="overflow-auto rounded-md bg-background px-3 py-2 font-mono text-xs whitespace-pre-wrap">{event.raw}</pre>}
        </div>
      )}
    </li>
  );
}
