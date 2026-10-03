import { useCallback, useEffect, useRef, useState } from "react";
import { errorText, serverLogs, serverRiders, serverTiming, type Rider, type ServerView } from "./api";
import { dedupePenalties, diffRiders, diffTiming, newLogLines, parseLogLine, prepend, withNames, type ParsedEvent, type ServerEvent, type Timing } from "./events";

const TIMING_MS = 1000;
const LOG_MS = 3000;
const RIDERS_MS = 3000;
const LOG_LINES = 500;

export interface EventFeed {
  events: ServerEvent[];
  errors: { log: string | null; timing: string | null };
}

/** Collects the server's events while mounted: the log tail, `/timing` and (with a token) the
 *  rider list, each polled, never two of one at once. */
export function useEventFeed(server: ServerView): EventFeed {
  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [logError, setLogError] = useState<string | null>(null);
  const [timingError, setTimingError] = useState<string | null>(null);
  const named = server.kind === "native" && (!server.local || (server.adminPort != null && server.hasToken));
  const seq = useRef(0);
  const names = useRef(new Map<number, string>());

  const add = useCallback((parsed: ParsedEvent[], source: ServerEvent["source"], backlog = false) => {
    if (parsed.length === 0) return;
    const at = Date.now();
    const fresh = parsed.map((p) => ({
      ...withNames(p, names.current),
      id: `${source}-${++seq.current}`,
      at,
      source,
      backlog: backlog || undefined,
    }));
    setEvents((buffer) => {
      const merged = dedupePenalties(buffer, fresh);
      return prepend(merged.buffer, merged.fresh);
    });
  }, []);

  useEffect(() => {
    if (server.kind !== "native") return;
    let alive = true;
    let lines: string[] | null = null;
    let timing: Timing | null = null;
    let riders: Rider[] | null = null;
    const busy = { log: false, timing: false, riders: false };

    const pollLog = async () => {
      if (busy.log) return;
      busy.log = true;
      try {
        const next = await serverLogs(server.id, LOG_LINES);
        if (!alive) return;
        const fresh = newLogLines(lines ?? [], next);
        const first = lines === null;
        lines = next;
        add(fresh.map(parseLogLine).filter((e): e is ParsedEvent => e !== null), "log", first);
        setLogError(null);
      } catch (e) {
        if (alive) setLogError(errorText(e));
      } finally {
        busy.log = false;
      }
    };
    const pollTiming = async () => {
      if (busy.timing) return;
      busy.timing = true;
      try {
        const next = await serverTiming(server.id);
        if (!alive) return;
        for (const entry of next.entries ?? []) names.current.set(entry.race, entry.name);
        add(diffTiming(timing, next), "timing");
        timing = next;
        setTimingError(null);
      } catch (e) {
        if (alive) setTimingError(errorText(e));
      } finally {
        busy.timing = false;
      }
    };
    const pollRiders = async () => {
      if (busy.riders) return;
      busy.riders = true;
      try {
        const next = await serverRiders(server.id);
        if (!alive) return;
        add(diffRiders(riders, next), "riders");
        riders = next;
      } catch {
        // The Riders tab says what is wrong with the token; joins still come from the log.
      } finally {
        busy.riders = false;
      }
    };

    void pollLog();
    void pollTiming();
    const timers = [window.setInterval(() => void pollLog(), LOG_MS), window.setInterval(() => void pollTiming(), TIMING_MS)];
    if (named) {
      void pollRiders();
      timers.push(window.setInterval(() => void pollRiders(), RIDERS_MS));
    }
    return () => {
      alive = false;
      timers.forEach((t) => window.clearInterval(t));
    };
  }, [server.id, server.kind, named, add]);

  return { events, errors: { log: logError, timing: timingError } };
}
