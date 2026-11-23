import { useMemo, useState, type UIEvent } from "react";
import { ChevronRight } from "lucide-react";
import type { ServerView } from "@/lib/api";
import { EVENT_KINDS, filterEvents, type EventKind, type FieldValue, type ServerEvent } from "@/lib/events";
import type { EventFeed } from "@/lib/useEventFeed";
import { ErrorLine, Input } from "./ui";

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

export function EventsTab({ server, feed }: { server: ServerView; feed: EventFeed }) {
  const [kinds, setKinds] = useState<Set<EventKind>>(() => new Set(DEFAULT_KINDS));
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  // Scrolled away from the top, the list holds still; back at the top, it follows again.
  const [held, setHeld] = useState<ServerEvent[] | null>(null);

  const source = held ?? feed.events;
  const shown = useMemo(() => filterEvents(source, kinds, query), [source, kinds, query]);

  if (server.kind === "legacy") return null;

  const toggle = (kind: EventKind) =>
    setKinds((current) => {
      const next = new Set(current);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });

  const onScroll = (e: UIEvent<HTMLDivElement>) => {
    const away = e.currentTarget.scrollTop > 4;
    if (away && !held) setHeld(feed.events);
    else if (!away && held) setHeld(null);
  };

  return (
    <div className="flex h-full flex-col gap-3">
      <div className="flex flex-wrap items-center gap-1.5">
        <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search riders" aria-label="Search riders" className="mr-2 w-56" />
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
            </button>
          );
        })}
      </div>

      {feed.errors.log && <ErrorLine text={feed.errors.log} />}
      {feed.errors.timing && <ErrorLine text={feed.errors.timing} />}

      <div className="min-h-0 flex-1 overflow-auto rounded-lg border bg-card" onScroll={onScroll}>
        {shown.length === 0 ? (
          <p className="p-4 text-sm text-muted-foreground">No events.</p>
        ) : (
          <ul className="divide-y">
            {shown.map((event) => (
              <EventRow key={event.id} event={event} open={open === event.id} onToggle={() => setOpen(open === event.id ? null : event.id)} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function EventRow({ event, open, onToggle }: { event: ServerEvent; open: boolean; onToggle: () => void }) {
  const fields = Object.entries(event.fields);
  return (
    <li>
      <button type="button" onClick={onToggle} aria-expanded={open} className="flex w-full items-center gap-3 px-3 py-2 text-left text-sm hover:bg-accent/40">
        <ChevronRight className={`size-3.5 shrink-0 text-muted-foreground transition ${open ? "rotate-90" : ""}`} aria-hidden />
        <span className="w-16 shrink-0 font-mono text-xs text-muted-foreground tabular-nums">
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
          {event.raw && <pre className="overflow-auto rounded-md bg-background px-3 py-2 font-mono text-xs whitespace-pre-wrap">{event.raw}</pre>}
        </div>
      )}
    </li>
  );
}
