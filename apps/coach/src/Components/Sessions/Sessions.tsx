import { useState } from "react";
import SessionList from "./SessionList";
import SessionView from "./SessionView";
import Review from "../Review/Review";

type Place =
  | { kind: "list" }
  | { kind: "session"; path: string }
  | { kind: "review"; path: string; lap: number; solo: boolean };

/** Sessions, one session's laps, and a lap's review: a drill-down with back links. */
export default function Sessions({ onSettings }: { onSettings: () => void }) {
  const [place, setPlace] = useState<Place>({ kind: "list" });

  if (place.kind === "review") {
    const { path, lap, solo } = place;
    return <Review path={path} lap={lap} solo={solo} onBack={() => setPlace({ kind: "session", path })} />;
  }
  if (place.kind === "session") {
    const { path } = place;
    return (
      <SessionView
        path={path}
        onBack={() => setPlace({ kind: "list" })}
        onReview={(lap, solo) => setPlace({ kind: "review", path, lap, solo })}
      />
    );
  }
  return <SessionList onOpen={(path) => setPlace({ kind: "session", path })} onSettings={onSettings} />;
}
