import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from "react";

export type Health = "ready" | "starting" | "down" | "unknown";

const healthStyle: Record<Health, { label: string; color: string }> = {
  ready: { label: "Ready", color: "var(--success)" },
  starting: { label: "Not ready", color: "var(--primary)" },
  down: { label: "Unreachable", color: "var(--destructive)" },
  unknown: { label: "Checking…", color: "var(--muted-foreground)" },
};

/** Colour always comes with its word, never alone. */
export function HealthBadge({ health }: { health: Health }) {
  const { label, color } = healthStyle[health];
  return (
    <span className="inline-flex items-center gap-1.5 text-sm font-medium" style={{ color }}>
      <span className="size-2 rounded-full" style={{ background: color }} aria-hidden />
      {label}
    </span>
  );
}

export function Button({
  variant = "secondary",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" | "danger" }) {
  const styles = {
    primary: "bg-primary text-primary-foreground hover:opacity-90",
    secondary: "bg-secondary text-secondary-foreground hover:bg-accent",
    danger: "bg-transparent text-destructive hover:bg-accent",
  }[variant];
  return (
    <button
      className={`inline-flex h-9 items-center gap-2 rounded-md px-3.5 text-sm font-medium transition disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-ring ${styles} ${className}`}
      {...props}
    />
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <label className="flex flex-col gap-1.5 text-sm">
      <span className="font-medium">{label}</span>
      {children}
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </label>
  );
}

export function Input(props: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className="h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/30"
      {...props}
    />
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
