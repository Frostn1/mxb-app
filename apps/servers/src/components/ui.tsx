import { useEffect, useRef, useState, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";
import { MoreHorizontal } from "lucide-react";
import type { StatusReport } from "@/lib/api";

/** One status for a server: Online · N riders, Starting…, Offline, Unreachable (SSH), or
 *  Checking… before the first answer. Colour always comes with its words; the detail is in
 *  the tooltip. */
export function StatusBadge({ report, error }: { report: StatusReport | null; error?: string | null }) {
  let label = "Checking…";
  let color = "var(--muted-foreground)";
  let detail = "Waiting for the first answer";
  if (error) {
    // The latest poll failed: never keep showing an older, happier answer.
    label = "Unreachable";
    color = "var(--destructive)";
    detail = error;
  } else if (report) {
    detail = report.detail;
    const riders = report.status?.active_sessions ?? 0;
    switch (report.state) {
      case "online":
        label = `Online · ${riders} ${riders === 1 ? "rider" : "riders"}`;
        color = "var(--success)";
        break;
      case "starting":
        label = "Starting…";
        color = "var(--primary)";
        break;
      case "offline":
        label = "Offline";
        color = "var(--muted-foreground)";
        break;
      case "unreachable":
        label = "Unreachable (SSH)";
        color = "var(--destructive)";
        break;
    }
  }
  return (
    <span className="inline-flex items-center gap-2 text-sm font-medium whitespace-nowrap" style={{ color }} title={detail}>
      <span className="size-2 rounded-full" style={{ background: color }} aria-hidden />
      {label}
    </span>
  );
}

export function Button({
  variant = "secondary",
  size = "md",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "danger" | "ghost";
  size?: "sm" | "md";
}) {
  const styles = {
    primary: "bg-primary text-primary-foreground hover:opacity-90",
    secondary: "bg-secondary text-secondary-foreground hover:bg-accent",
    danger: "bg-transparent text-destructive hover:bg-accent",
    ghost: "bg-transparent text-muted-foreground hover:bg-accent hover:text-foreground",
  }[variant];
  const sizes = size === "sm" ? "h-8 px-2.5 text-xs" : "h-9 px-3.5 text-sm";
  return (
    <button
      className={`inline-flex items-center justify-center gap-2 rounded-md font-medium transition disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-ring ${sizes} ${styles} ${className}`}
      {...props}
    />
  );
}

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
  /** Asked before `onSelect` runs. */
  confirm?: string;
}

/** A ⋯ button with a small menu. */
export function OverflowMenu({ items, label = "More actions" }: { items: MenuItem[]; label?: string }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const shut = (refocus: boolean) => {
    setOpen(false);
    if (refocus) trigger.current?.focus();
  };
  useEffect(() => {
    if (!open) return;
    // Keyboard users land on the first item.
    list.current?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
    const close = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  const onKey = (e: React.KeyboardEvent) => {
    const buttons = [...(list.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? [])];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      shut(true);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      buttons[(at + 1) % buttons.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      buttons[(at - 1 + buttons.length) % buttons.length]?.focus();
    }
  };
  return (
    <div className="relative" ref={box}>
      <button
        ref={trigger}
        type="button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(e) => {
          e.stopPropagation();
          setOpen(!open);
        }}
        className="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
      >
        <MoreHorizontal className="size-4" />
      </button>
      {open && (
        <div ref={list} role="menu" aria-label={label} onKeyDown={onKey} className="absolute right-0 z-20 mt-1 min-w-36 rounded-lg border bg-popover p-1 text-popover-foreground shadow-lg">
          {items.map((item) => (
            <button
              type="button"
              key={item.label}
              role="menuitem"
              onClick={(e) => {
                e.stopPropagation();
                shut(true);
                if (item.confirm && !window.confirm(item.confirm)) return;
                item.onSelect();
              }}
              className={`block w-full rounded-md px-3 py-1.5 text-left text-sm hover:bg-accent ${item.danger ? "text-destructive" : ""}`}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5 text-sm">
      <span className="font-medium">{label}</span>
      {children}
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </label>
  );
}

export function Input({ className = "", ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={`h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/30 ${className}`}
      {...props}
    />
  );
}

/** An on/off switch. */
export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative inline-flex h-6 w-10 shrink-0 items-center rounded-full transition focus-visible:outline-2 focus-visible:outline-ring ${
        checked ? "bg-primary" : "bg-secondary ring-1 ring-border ring-inset"
      }`}
    >
      <span
        className={`inline-block size-5 rounded-full bg-white shadow transition ${checked ? "translate-x-[1.1rem]" : "translate-x-0.5"}`}
      />
    </button>
  );
}

export function Stat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="font-mono text-base tabular-nums">{value}</span>
    </div>
  );
}

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className={`rounded-xl border bg-card p-5 text-card-foreground ${className}`}>{children}</div>;
}

export function ErrorLine({ text }: { text: string }) {
  return <p className="text-sm break-words text-destructive">{text}</p>;
}

/** A tinted box for guidance or a result. */
export function Notice({ tone = "info", children }: { tone?: "info" | "ok" | "bad"; children: ReactNode }) {
  const color = tone === "ok" ? "var(--success)" : tone === "bad" ? "var(--destructive)" : "var(--primary)";
  return (
    <div className="rounded-lg border-l-4 bg-muted px-4 py-3 text-sm" style={{ borderLeftColor: color }}>
      {children}
    </div>
  );
}
