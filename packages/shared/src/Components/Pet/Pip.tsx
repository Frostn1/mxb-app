/**
 * Pip, the MXB pet: a mud chick with its goggles pushed up. The same character FrostMod draws
 * running Coach's line in game (`coachpet.h` there, as pixel art); this is the app's drawing of it.
 *
 * Pure SVG in a 100 x 100 box, so it scales to any size and follows no theme: Pip is yellow
 * everywhere.
 */

export type PipFace = "idle" | "blink" | "happy" | "wow" | "scared" | "sleep";

const EYE = "#1b1b1b";

function Face({ face }: { face: PipFace }) {
  const y = 52, dx = 12, cx = 50, my = y + 13;
  const smile = <path d={`M${cx - 4} ${my} q4 3.5 8 0`} stroke={EYE} strokeWidth={2} fill="none" strokeLinecap="round" />;
  switch (face) {
    case "blink":
      return (
        <>
          <path d={`M${cx - dx - 4} ${y} h8 M${cx + dx - 4} ${y} h8`} stroke={EYE} strokeWidth={2.4} strokeLinecap="round" />
          {smile}
        </>
      );
    case "happy":
      return (
        <>
          <ellipse cx={cx - dx - 5} cy={y + 8} rx={4} ry={2.4} fill="#ff7a8a" opacity={0.55} />
          <ellipse cx={cx + dx + 5} cy={y + 8} rx={4} ry={2.4} fill="#ff7a8a" opacity={0.55} />
          <path
            d={`M${cx - dx - 4.5} ${y + 1.5} q4.5 -6 9 0 M${cx + dx - 4.5} ${y + 1.5} q4.5 -6 9 0`}
            stroke={EYE}
            strokeWidth={2.6}
            fill="none"
            strokeLinecap="round"
          />
          <path d={`M${cx - 7} ${my - 2} q7 10 14 0 z`} fill="#7a2230" />
          <path d={`M${cx - 3.5} ${my + 2.4} q3.5 2 7 0`} fill="#ff8a9a" />
        </>
      );
    case "wow":
      return (
        <>
          <path d="M18 22 l3 6 l6 -3 l-6 -3 z M80 20 l3 5 l5 -3 l-5 -2 z" fill="#ffd23f" />
          {[-dx, dx].map((o) => (
            <g key={o}>
              <circle cx={cx + o} cy={y} r={6} fill={EYE} />
              <circle cx={cx + o + 2} cy={y - 2} r={2.2} fill="#fff" />
              <circle cx={cx + o - 2} cy={y + 2.4} r={1} fill="#fff" />
            </g>
          ))}
          <ellipse cx={cx} cy={my + 1} rx={4} ry={5} fill="#7a2230" />
        </>
      );
    case "scared":
      return (
        <>
          <path d="M80 30 q4 7 0 10 q-4 -3 0 -10 z" fill="#7fd0ff" />
          {[-dx, dx].map((o) => (
            <g key={o}>
              <circle cx={cx + o} cy={y} r={6} fill="#fff" stroke={EYE} strokeWidth={1.6} />
              <circle cx={cx + o} cy={y} r={1.8} fill={EYE} />
            </g>
          ))}
          <path
            d={`M${cx - 7} ${my + 1} l3.5 -2.5 l3.5 2.5 l3.5 -2.5 l3.5 2.5`}
            stroke={EYE}
            strokeWidth={2}
            fill="none"
            strokeLinejoin="round"
          />
        </>
      );
    case "sleep":
      return (
        <>
          <path
            d={`M${cx - dx - 4.5} ${y} q4.5 4 9 0 M${cx + dx - 4.5} ${y} q4.5 4 9 0`}
            stroke={EYE}
            strokeWidth={2.4}
            fill="none"
            strokeLinecap="round"
          />
          <ellipse cx={cx} cy={my + 1} rx={2} ry={2.4} fill={EYE} opacity={0.7} />
        </>
      );
    default:
      return (
        <>
          {[-dx, dx].map((o) => (
            <g key={o}>
              <ellipse cx={cx + o} cy={y} rx={3.6} ry={4.6} fill={EYE} />
              <circle cx={cx + o + 1.2} cy={y - 1.6} r={1.2} fill="#fff" />
            </g>
          ))}
          {smile}
        </>
      );
  }
}

export default function Pip({ face = "idle", className }: { face?: PipFace; className?: string }) {
  return (
    <svg viewBox="0 0 100 100" className={className} overflow="visible" aria-hidden="true">
      <ellipse cx={50} cy={94} rx={24} ry={4} fill="#000" opacity={0.2} />
      <path d="M40 84 l0 9 h6 M58 84 l0 9 h6" stroke="#e8761a" strokeWidth={4} strokeLinecap="round" fill="none" />
      <ellipse cx={50} cy={58} rx={33} ry={30} fill="#ffc93c" />
      <ellipse cx={50} cy={66} rx={22} ry={17} fill="#ffe48a" />
      <path d="M21 66 q-9 -2 -10 -10 q8 0 12 4" fill="#f2b21f" />
      <path d="M79 66 q9 -2 10 -10 q-8 0 -12 4" fill="#f2b21f" />
      <path d="M44 22 q4 -12 8 0 q4 -10 7 1" stroke="#f2b21f" strokeWidth={4} fill="none" strokeLinecap="round" />
      <rect x={22} y={31} width={56} height={9} rx={4.5} fill="#2b2b2b" />
      <ellipse cx={38} cy={35} rx={9} ry={7} fill="#ff7a1a" stroke="#2b2b2b" strokeWidth={2.5} />
      <ellipse cx={62} cy={35} rx={9} ry={7} fill="#ff7a1a" stroke="#2b2b2b" strokeWidth={2.5} />
      <path d="M34 33 l5 -2 M58 33 l5 -2" stroke="#fff" strokeWidth={2} opacity={0.7} strokeLinecap="round" />
      <path d="M30 76 q4 3 8 1 M64 79 q4 1 7 -2" stroke="#8a5a2b" strokeWidth={3} opacity={0.55} fill="none" strokeLinecap="round" />
      <Face face={face} />
    </svg>
  );
}
