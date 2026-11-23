import { useEffect, useMemo, useReducer, useRef, useState, type RefObject } from "react";
import { useThree } from "@react-three/fiber";
import { Keyboard, Move, Rotate3d, ZoomIn, type LucideIcon } from "lucide-react";
import * as THREE from "three";
import { cn } from "../../lib/utils";
import { useT } from "../../i18n/context";
import {
  PIE_ITEMS,
  ViewportNav,
  pieItemAt,
  useNavStyle,
  type OrbitLike,
  type ViewName,
} from "../../lib/viewportNav";

/** The viewers' gesture legend, in the navigation style that's on. */
export function useNavHintItems(): { Icon: LucideIcon; label: string }[] {
  const t = useT();
  const [style] = useNavStyle();
  return style === "blender"
    ? [
        { Icon: Rotate3d, label: t("viewer.nav.middleOrbit") },
        { Icon: Move, label: t("viewer.nav.shiftPan") },
        { Icon: ZoomIn, label: t("viewer.scrollToZoom") },
        { Icon: Keyboard, label: t("viewer.nav.views") },
      ]
    : [
        { Icon: Rotate3d, label: t("viewer.dragToRotate") },
        { Icon: ZoomIn, label: t("viewer.scrollToZoom") },
        { Icon: Move, label: t("viewer.rightDragToPan") },
      ];
}

/**
 * Hooks a react-three-fiber viewer up to {@link ViewportNav}. Goes inside the `<Canvas>`,
 * after the `OrbitControls` (`makeDefault`); hands the controller out through `onNav` so the
 * overlay, which lives outside the canvas, can draw the gizmo and the pie.
 */
export function NavBridge({
  container,
  frameRoot,
  onNav,
}: {
  /** The element around the canvas: input is read off it. */
  container: RefObject<HTMLElement>;
  /** What "frame all" fits — the model, not the sky dome around it. */
  frameRoot?: RefObject<THREE.Object3D>;
  onNav: (nav: ViewportNav | null) => void;
}) {
  const camera = useThree((s) => s.camera);
  const controls = useThree((s) => s.controls) as unknown as OrbitLike | null;
  const invalidate = useThree((s) => s.invalidate);
  const report = useRef(onNav);
  report.current = onNav;
  useEffect(() => {
    const el = container.current;
    const cam = camera as THREE.PerspectiveCamera;
    if (!el || !controls || !("fov" in cam)) return;
    const nav = new ViewportNav({
      camera: cam,
      controls,
      element: el,
      onChange: () => invalidate(),
      getFrameBox: () => {
        const root = frameRoot?.current;
        if (!root) return null;
        root.updateWorldMatrix(true, true);
        const box = new THREE.Box3().setFromObject(root);
        return box.isEmpty() ? null : box;
      },
    });
    report.current(nav);
    return () => {
      report.current(null);
      nav.dispose();
    };
  }, [camera, controls, invalidate, container, frameRoot]);
  return null;
}

/** Re-render whenever the controller says the camera, style or pie changed. */
function useNav(nav: ViewportNav | null) {
  const [, bump] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    if (!nav) return;
    let raf = 0;
    const off = nav.subscribe(() => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        bump();
      });
    });
    return () => {
      off();
      if (raf) cancelAnimationFrame(raf);
    };
  }, [nav]);
}

// Blender's axis colours.
const AXES: { label: string; color: string; pos: ViewName; neg: ViewName; v: [number, number, number] }[] = [
  // Blender X is our X; clicking +X looks from +X: the right view.
  { label: "X", color: "#ff3352", pos: "right", neg: "left", v: [1, 0, 0] },
  // Blender Y is our −Z; looking from +Y is the back view.
  { label: "Y", color: "#8bdc00", pos: "back", neg: "front", v: [0, 0, -1] },
  // Blender Z is our Y; from +Z is the top.
  { label: "Z", color: "#2890ff", pos: "top", neg: "bottom", v: [0, 1, 0] },
];

const SIZE = 84;
const R = 30;
const BALL = 8;

/**
 * Blender's navigation gizmo: the three axes as they face the camera, a ball per end. Click
 * a ball to look along that axis (again for the far side), drag anywhere on it to orbit.
 * Plus the perspective/ortho switch under it, as Blender has.
 */
function NavGizmo({ nav }: { nav: ViewportNav }) {
  const t = useT();
  const [hot, setHot] = useState<string | null>(null);
  const drag = useRef<{ x: number; y: number; moved: boolean; id: number; view: ViewName | null } | null>(null);
  const inv = nav.camera.quaternion.clone().invert();
  const balls = AXES.flatMap((a) =>
    [1, -1].map((sign) => {
      const v = new THREE.Vector3(...a.v).multiplyScalar(sign).applyQuaternion(inv);
      return {
        key: `${sign > 0 ? "+" : "-"}${a.label}`,
        label: a.label,
        color: a.color,
        view: sign > 0 ? a.pos : a.neg,
        positive: sign > 0,
        x: SIZE / 2 + v.x * R,
        y: SIZE / 2 - v.y * R,
        z: v.z,
      };
    }),
  );
  // Far side first, so the near balls draw over it.
  balls.sort((a, b) => a.z - b.z);

  const onDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    // The ball is read here, off what was pressed: once the svg captures the pointer, the
    // release is targeted at the svg itself and no longer says which ball it was over.
    const view = (e.target as Element).closest?.("[data-view]")?.getAttribute("data-view") as ViewName | null;
    drag.current = { x: e.clientX, y: e.clientY, moved: false, id: e.pointerId, view: view ?? null };
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
  };
  const onMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d || d.id !== e.pointerId) return;
    const dx = e.clientX - d.x;
    const dy = e.clientY - d.y;
    if (!d.moved && Math.hypot(dx, dy) < 3) return;
    d.moved = true;
    d.x = e.clientX;
    d.y = e.clientY;
    nav.orbit(dx, dy);
  };
  const onUp = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d || d.id !== e.pointerId) return;
    drag.current = null;
    (e.currentTarget as Element).releasePointerCapture?.(e.pointerId);
    if (!d.moved && d.view) nav.viewAxis(d.view);
  };
  // Blender's header names the view: "Right Orthographic", "User Perspective".
  const view = nav.currentView();
  const projection = nav.isOrtho ? t("viewer.view.ortho") : t("viewer.view.persp");

  return (
    <div className="flex flex-col items-center gap-1">
      <svg
        width={SIZE}
        height={SIZE}
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        className="cursor-pointer touch-none select-none rounded-full transition-colors hover:bg-white/10"
        role="img"
        aria-label={t("viewer.view.gizmo")}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={() => (drag.current = null)}
        onPointerLeave={() => setHot(null)}
      >
        <title>{t("viewer.view.gizmo")}</title>
        {balls
          .filter((b) => b.positive)
          .map((b) => (
            <line
              key={`l${b.key}`}
              x1={SIZE / 2}
              y1={SIZE / 2}
              x2={b.x}
              y2={b.y}
              stroke={b.color}
              strokeWidth={2}
              strokeLinecap="round"
              opacity={0.9}
            />
          ))}
        {balls.map((b) => (
          <g
            key={b.key}
            data-view={b.view}
            data-axis={b.key}
            onPointerEnter={() => setHot(b.key)}
            onPointerLeave={() => setHot((h) => (h === b.key ? null : h))}
          >
            <title>{t(`viewer.view.${b.view}`)}</title>
            <circle
              cx={b.x}
              cy={b.y}
              r={hot === b.key ? BALL + 1.5 : BALL}
              fill={b.color}
              fillOpacity={b.positive ? 1 : hot === b.key ? 0.8 : 0.35}
              stroke={hot === b.key ? "#ffffff" : b.positive ? "none" : b.color}
              strokeWidth={hot === b.key ? 2 : 1.5}
            />
            {(b.positive || hot === b.key) && (
              <text
                x={b.x}
                y={b.y + 3.5}
                textAnchor="middle"
                fontSize={10}
                fontWeight={700}
                fill={b.positive ? "#101010" : "#ffffff"}
                className="pointer-events-none"
              >
                {b.positive ? b.label : `-${b.label}`}
              </text>
            )}
          </g>
        ))}
      </svg>
      <button
        type="button"
        onClick={() => nav.run({ kind: "toggleOrtho" })}
        title={t("viewer.view.toggleOrtho")}
        data-nav-projection
        className="rounded bg-black/35 px-1.5 py-0.5 text-[10px] font-medium leading-none text-white/75 hover:bg-black/55 hover:text-white"
      >
        {view ? `${t(`viewer.view.${view}`)} ${projection}` : projection}
      </button>
    </div>
  );
}

const PIE_R = 96;

/** The backtick's view pie, centred where the pointer was. */
function NavPie({ nav }: { nav: ViewportNav }) {
  const t = useT();
  const pie = nav.pie!;
  const [aim, setAim] = useState<string | null>(null);
  const el = nav.element;
  // Hold-and-release: let go of the backtick while pointing at a slot to pick it, as in
  // Blender. A quick tap leaves the pie open for a click.
  useEffect(() => {
    let at = { x: pie.x, y: pie.y };
    const onMove = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      at = { x: e.clientX - r.left, y: e.clientY - r.top };
      setAim(pieItemAt(at.x - pie.x, at.y - pie.y)?.id ?? null);
    };
    const onUp = (e: KeyboardEvent) => {
      if (e.code !== "Backquote") return;
      const it = pieItemAt(at.x - pie.x, at.y - pie.y);
      if (it) nav.pick(it.id);
    };
    el.addEventListener("pointermove", onMove);
    window.addEventListener("keyup", onUp);
    return () => {
      el.removeEventListener("pointermove", onMove);
      window.removeEventListener("keyup", onUp);
    };
  }, [nav, el, pie.x, pie.y]);
  const label = (id: (typeof PIE_ITEMS)[number]["id"]) =>
    id === "frameAll"
      ? t("viewer.view.frameAll")
      : id === "frameSelected"
        ? t("viewer.view.frameSelected")
        : t(`viewer.view.${id}`);
  return (
    <div className="pointer-events-none absolute inset-0 z-20" data-nav-pie>
      <div
        className="absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full border border-white/60 bg-black/40"
        style={{ left: pie.x, top: pie.y }}
      />
      {PIE_ITEMS.map((it) => {
        const a = (it.angle * Math.PI) / 180;
        return (
          <button
            key={it.id}
            type="button"
            data-nav-pie
            onPointerEnter={() => setAim(it.id)}
            onClick={() => nav.pick(it.id)}
            className={cn(
              "pointer-events-auto absolute -translate-x-1/2 -translate-y-1/2 whitespace-nowrap rounded-full border px-2.5 py-1 text-[11px] font-medium leading-none shadow-md",
              aim === it.id
                ? "border-white/40 bg-[#3d6fd8] text-white"
                : "border-white/15 bg-[#1f2024]/95 text-white/85 hover:bg-[#2a2c31]",
            )}
            style={{ left: pie.x + Math.cos(a) * PIE_R, top: pie.y + Math.sin(a) * PIE_R * 0.8 }}
          >
            {label(it.id)}
          </button>
        );
      })}
    </div>
  );
}

/**
 * The viewport's navigation chrome: the axis gizmo in a corner and, while it's open, the
 * view pie. Place inside the same positioned element the viewer fills.
 */
export function ViewportNavOverlay({
  nav,
  className,
  hideGizmo = false,
}: {
  nav: ViewportNav | null;
  className?: string;
  /** No room for it (a docked thumbnail-sized preview): the keys and the pie still work. */
  hideGizmo?: boolean;
}) {
  useNav(nav);
  const pieKey = useMemo(() => (nav?.pie ? `${nav.pie.x},${nav.pie.y}` : null), [nav?.pie]);
  if (!nav) return null;
  return (
    <>
      {!hideGizmo && (
        // Below the corner rather than in it: the viewers' hosts put their own buttons and
        // notices in the top-right corner (fullscreen, reload notices, track stats).
        <div className={cn("absolute right-2 top-12 z-10", className)}>
          <NavGizmo nav={nav} />
        </div>
      )}
      {nav.pie && <NavPie key={pieKey} nav={nav} />}
    </>
  );
}
