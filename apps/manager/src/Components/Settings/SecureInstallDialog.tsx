import { useEffect, useMemo, useState } from "react";
import {
  scanBikeTargets,
  scanRiderTargets,
  type RiderTargets,
  type SecureInstallInfo,
  type SecureInstallTarget,
} from "@frost/shared/api/mods";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@frost/shared/Components/ui/alert-dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@frost/shared/Components/ui/select";
import { useT, type TKey } from "@/i18n";

/** Where a paint can go, and the list of models each one is picked from. */
const PAINT_AREAS: { area: string; label: TKey }[] = [
  { area: "bike", label: "settings.secInstallBike" },
  { area: "helmet", label: "settings.secInstallHelmet" },
  { area: "goggles", label: "settings.secInstallGoggles" },
  { area: "boots", label: "settings.secInstallBoots" },
  { area: "suit", label: "settings.secInstallSuit" },
  { area: "gloves", label: "settings.secInstallGloves" },
];

/** Where a package (a `.pkz`) can go. */
const PACKAGE_AREAS: { area: string; label: TKey }[] = [
  { area: "tracks", label: "settings.secInstallTracks" },
  { area: "bikes", label: "settings.secInstallBikes" },
  { area: "helmets", label: "settings.secInstallHelmets" },
  { area: "boots", label: "settings.secInstallBootsPkg" },
  { area: "riders", label: "settings.secInstallRiders" },
];

function modelsFor(area: string, bikes: string[], rider: RiderTargets | null): string[] {
  if (area === "bike") return bikes;
  if (area === "helmet" || area === "goggles") return rider?.helmets ?? [];
  if (area === "boots") return rider?.boots ?? [];
  if (area === "suit" || area === "gloves") return rider?.profiles ?? [];
  return [];
}

/**
 * Asks where a secured file picked from outside the mods folder belongs, since the game only
 * reads content from there. A paint needs the bike (or gear) it is for; a package needs its
 * content folder. The unlock runs only after this is answered.
 */
export default function SecureInstallDialog({
  info,
  onCancel,
  onConfirm,
}: {
  info: SecureInstallInfo | null;
  onCancel: () => void;
  onConfirm: (target: SecureInstallTarget) => void;
}) {
  const t = useT();
  const paint = info?.kind === "paint";
  const areas = paint ? PAINT_AREAS : PACKAGE_AREAS;
  const [area, setArea] = useState(areas[0].area);
  const [name, setName] = useState("");
  const [bikes, setBikes] = useState<string[]>([]);
  const [rider, setRider] = useState<RiderTargets | null>(null);

  useEffect(() => {
    if (!info) return;
    setArea((info.kind === "paint" ? PAINT_AREAS : PACKAGE_AREAS)[0].area);
    setName("");
    if (info.kind !== "paint") return;
    scanBikeTargets().then(setBikes).catch(() => setBikes([]));
    scanRiderTargets().then(setRider).catch(() => setRider(null));
  }, [info]);

  const models = useMemo(() => modelsFor(area, bikes, rider), [area, bikes, rider]);
  const ready = !!info && (!paint || models.some((m) => m === name));

  return (
    <AlertDialog open={!!info} onOpenChange={(open) => !open && onCancel()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("settings.secInstallTitle")}</AlertDialogTitle>
          <AlertDialogDescription>
            {t(paint ? "settings.secInstallPaintDesc" : "settings.secInstallPackageDesc", {
              name: info?.gameName ?? "",
            })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="flex flex-col gap-3">
          <Select
            value={area}
            onValueChange={(v) => {
              setArea(v);
              setName("");
            }}
          >
            <SelectTrigger className="h-8 w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {areas.map((a) => (
                <SelectItem key={a.area} value={a.area}>
                  {t(a.label)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {paint &&
            (models.length > 0 ? (
              <Select value={name} onValueChange={setName}>
                <SelectTrigger className="h-8 w-full">
                  <SelectValue placeholder={t("settings.secInstallPick")} />
                </SelectTrigger>
                <SelectContent>
                  {models.map((m) => (
                    <SelectItem key={m} value={m}>
                      {m}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <p className="text-[12px] text-muted-foreground">{t("settings.secInstallNone")}</p>
            ))}
        </div>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
          <AlertDialogAction
            disabled={!ready}
            onClick={() => onConfirm(paint ? { area, name } : { area })}
          >
            {t("settings.secInstallGo")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
