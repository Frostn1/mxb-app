import { useEffect, useState } from "react";
import SessionList from "./SessionList";
import SessionView from "./SessionView";
import Review from "../Review/Review";
import { track } from "@/lib/analytics";

type Place =
  | { kind: "list" }
  | { kind: "session"; path: string }
  // `path` is the lap's own recording; `session` is the session it belongs to, for the way
  // back. `trackId` is carried along because the reference is remembered per track.
  | { kind: "review"; session: string; path: string; lap: number; solo: boolean; trackId: string };

/** Sessions, one session's laps, and a lap's review: a drill-down with back links. */
export default function Sessions({ onSettings }: { onSettings: () => void }) {
  const [place, setPlace] = useState<Place>({ kind: "list" });

  // Counted off where the drill-down *is*, not off the handlers that move it — a back link and
  // a fresh pick both land here, and a step nobody counted is worse than one counted twice. The
  // list itself is the Sessions page, which `App` already counts as `view.sessions`.
  useEffect(() => {
    if (place.kind === "session") track("coach.session.open");
    else if (place.kind === "review") track("coach.review");
  }, [place.kind]);

  if (place.kind === "review") {
    const { session, path, lap, solo, trackId } = place;
    return (
      <Review path={path} lap={lap} trackId={trackId} solo={solo} onBack={() => setPlace({ kind: "session", path: session })} />
    );
  }
  if (place.kind === "session") {
    const { path } = place;
    return (
      <SessionView
        path={path}
        onBack={() => setPlace({ kind: "list" })}
        onReview={(file, lap, solo, trackId) => setPlace({ kind: "review", session: path, path: file, lap, solo, trackId })}
      />
    );
  }
  return <SessionList onOpen={(path) => setPlace({ kind: "session", path })} onSettings={onSettings} />;
}
