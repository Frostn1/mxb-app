import type { ReactNode } from "react";
import { Switch } from "@frost/shared/Components/ui/switch";
import { cn } from "@frost/shared/lib/utils";

/* MXB App's Settings card and switch row (apps/manager Settings.tsx), so the two apps' Settings
   read the same. Kept in step with those by hand: change both or neither. */

/** One card on a Settings page: a bold title, an optional line under it, then the rows. */
export function Section({
  title,
  desc,
  titleRight,
  children,
}: {
  title: string;
  desc?: string;
  titleRight?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-3 rounded-xl bg-card p-[18px]">
      <div className="flex items-center gap-2">
        <span className="flex-1 text-[14px] font-bold">{title}</span>
        {titleRight}
      </div>
      {desc && <span className="-mt-1.5 text-[12px] text-muted-foreground">{desc}</span>}
      {children}
    </div>
  );
}

/** A label, an optional line under it, and a switch on the right. */
export function ToggleRow({
  label,
  desc,
  checked,
  onChange,
  disabled = false,
  children,
}: {
  label: string;
  desc?: ReactNode;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  /** Notes under the description, such as a recorder that is too old for it. */
  children?: ReactNode;
}) {
  return (
    <div className={cn("flex items-start justify-between gap-4", disabled && "opacity-60")}>
      <div className="flex flex-col gap-0.5">
        <span className="text-[12.5px] text-foreground/85">{label}</span>
        {desc && <span className="text-[11.5px] leading-relaxed text-muted-foreground">{desc}</span>}
        {children}
      </div>
      <div className="pt-0.5">
        <Switch checked={checked} onCheckedChange={onChange} disabled={disabled} />
      </div>
    </div>
  );
}

/** A label on the left and a control on the right, on one line. */
export function FieldRow({ label, desc, children }: { label: string; desc?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-[12.5px] text-foreground/85">{label}</span>
        {desc && <span className="text-[11.5px] leading-relaxed text-muted-foreground">{desc}</span>}
      </div>
      {children}
    </div>
  );
}

/** The thin rule MXB App puts between groups of rows inside a card. */
export const Rule = () => <div className="h-px bg-border" />;
