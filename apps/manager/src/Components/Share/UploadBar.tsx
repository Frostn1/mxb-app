import { Progress } from "@frost/shared/Components/ui/progress";
import type { BundleProgress } from "@frost/shared/types";
import { useT } from "@/i18n";

/**
 * How far a share upload has got, counted in stored parts. A one-part upload has nothing
 * to count, so its bar runs indeterminate rather than sitting at 0% until it lands.
 */
export function UploadBar({ progress }: { progress: BundleProgress | null }) {
  const t = useT();
  if (progress?.phase !== "uploading" || !progress.total) return null;
  const { done = 0, total } = progress;
  const pct = total > 1 ? (done / total) * 100 : undefined;
  return (
    <div className="flex flex-col gap-1">
      <Progress value={pct} className="h-1.5 rounded-full" barClassName="rounded-full" />
      {pct !== undefined && (
        <div className="flex justify-between text-[11px] text-muted-foreground">
          <span>{t("share.partsUploaded", { done, total })}</span>
          <span>{Math.round(pct)}%</span>
        </div>
      )}
    </div>
  );
}
