import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import Pip, { type PipFace } from "@frost/shared/Components/Pet/Pip";

/**
 * Pip idling in the bottom-right corner: breathing, blinking, looking about, asleep after a
 * couple of minutes with nobody at the app, happy when clicked. The free pet; the same one runs
 * Coach's line in game.
 *
 * Under the toasts and the support prompt (z-40 against their z-50), and only Pip itself takes
 * the mouse, so it never blocks what is behind its corner. Settings → Appearance hides it.
 */

const KEY = "mxb.pet";
const EVENT = "mxb-pet-change";
const SLEEP_AFTER_MS = 120_000;

function read(): boolean {
  try {
    return localStorage.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}

/** Whether Pip shows. On until turned off; kept per machine. */
export function usePetOn(): [boolean, (on: boolean) => void] {
  const on = useSyncExternalStore(
    (cb) => {
      window.addEventListener(EVENT, cb);
      window.addEventListener("storage", cb);
      return () => {
        window.removeEventListener(EVENT, cb);
        window.removeEventListener("storage", cb);
      };
    },
    read,
  );
  const set = (v: boolean) => {
    try {
      localStorage.setItem(KEY, v ? "on" : "off");
    } catch {
      // No storage: it still changes for this run.
    }
    window.dispatchEvent(new Event(EVENT));
  };
  return [on, set];
}

export default function PetCorner() {
  const [on] = usePetOn();
  const [face, setFace] = useState<PipFace>("idle");
  const [look, setLook] = useState(0);
  const [hearts, setHearts] = useState<number[]>([]);
  const lastActive = useRef(Date.now());
  const happyUntil = useRef(0);

  useEffect(() => {
    if (!on) return;
    const wake = () => {
      lastActive.current = Date.now();
    };
    window.addEventListener("pointermove", wake);
    window.addEventListener("keydown", wake);
    let blinkUntil = 0;
    const id = window.setInterval(() => {
      const now = Date.now();
      if (now < happyUntil.current) return setFace("happy");
      if (now - lastActive.current > SLEEP_AFTER_MS) return setFace("sleep");
      if (now >= blinkUntil && Math.random() < 0.06) blinkUntil = now + 150;
      setFace(now < blinkUntil ? "blink" : "idle");
      // Now and then a glance one way or the other.
      if (Math.random() < 0.01) setLook((Math.random() < 0.5 ? -1 : 1) * 6);
      else if (Math.random() < 0.03) setLook(0);
    }, 100);
    return () => {
      window.clearInterval(id);
      window.removeEventListener("pointermove", wake);
      window.removeEventListener("keydown", wake);
    };
  }, [on]);

  if (!on) return null;

  const pet = () => {
    happyUntil.current = Date.now() + 1500;
    lastActive.current = Date.now();
    setFace("happy");
    const id = Date.now();
    setHearts((h) => [...h, id]);
    window.setTimeout(() => setHearts((h) => h.filter((x) => x !== id)), 1100);
  };

  return (
    <div className="pointer-events-none fixed bottom-3 right-4 z-40 size-16 select-none">
      <style>{`
        @keyframes pip-breathe { 0%,100% { transform: scale(1,1) } 50% { transform: scale(1.03,0.97) } }
        @keyframes pip-heart { from { transform: translateY(0); opacity: 1 } to { transform: translateY(-40px); opacity: 0 } }
      `}</style>
      {face === "sleep" && (
        <span className="absolute -top-3 right-0 text-[12px] font-extrabold text-muted-foreground">z z</span>
      )}
      {hearts.map((id) => (
        <span key={id} className="absolute -top-1 left-1/2 text-[15px] text-[#ff4d6d]" style={{ animation: "pip-heart 1s ease-out forwards" }}>
          ♥
        </span>
      ))}
      <button
        type="button"
        aria-label="Pip"
        onClick={pet}
        className="pointer-events-auto block size-full cursor-default transition-transform duration-300"
        style={{ transform: `translateX(${look}px)` }}
      >
        <span className="block size-full origin-bottom" style={{ animation: "pip-breathe 2.4s ease-in-out infinite" }}>
          <Pip face={face} className="size-full" />
        </span>
      </button>
    </div>
  );
}
