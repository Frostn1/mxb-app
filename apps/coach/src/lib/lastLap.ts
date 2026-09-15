import { coachSessions, type SessionSummary } from "@/api/coach";

export interface LastLap {
  session: SessionSummary;
  /** The lap's number, as the review takes it. */
  lap: number;
}

/** The newest session's last whole, valid lap; an older session's when the newest has none. */
export function pickLastLap(sessions: SessionSummary[]): LastLap | null {
  const newest = [...sessions].sort((a, b) => b.started.localeCompare(a.started));
  for (const session of newest) {
    const lap = [...session.laps].reverse().find((l) => l.whole && !l.invalid);
    if (lap) return { session, lap: lap.num };
  }
  return null;
}

/** The lap the overlay coaches: the last one ridden that can be reviewed. */
export async function lastLap(): Promise<LastLap | null> {
  return pickLastLap(await coachSessions());
}
