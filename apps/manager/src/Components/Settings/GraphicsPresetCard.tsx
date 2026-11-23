import { useCallback, useEffect, useState } from "react";
import { LoaderCircle, Info, TriangleAlert, Camera, FolderOpen, ImageDown } from "lucide-react";
import { toast } from "sonner";
import {
  convertHiresShots,
  graphicsPresetStatus,
  openPhotoFolder,
  setGraphicsPreset,
} from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import type {
  GraphicsExisting,
  GraphicsPreset,
  GraphicsPresetStatus,
} from "@frost/shared/types";
import { useConfig } from "@frost/shared/Context/Config";
import { Segmented } from "@frost/shared/Components/ui/segmented";
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
import { useGameRunning } from "../../lib/useGameRunning";
import { useT, type TFunc, type TKey } from "@/i18n";

/** The preset names are product names, shown as-is in every language. */
const LABEL: Record<GraphicsPreset, string> = {
  off: "",
  clean: "MXB Clean",
  photo: "MXB Photo",
};

const DESC: Record<GraphicsPreset, TKey> = {
  off: "graphics.offDesc",
  clean: "graphics.cleanDesc",
  photo: "graphics.photoDesc",
};

/** What the player already has, in one phrase, for the warning and the confirm dialog. */
export function describeExisting(
  e: GraphicsExisting,
  t: TFunc<TKey>,
): string {
  if (e.reshade && e.version) return t("graphics.existingReshadeVersion", { version: e.version });
  if (e.reshade) return t("graphics.existingReshade");
  if (e.otherDll) return t("graphics.existingOtherDll");
  return t("graphics.existingConfig");
}

/**
 * The Graphics preset setting: Off / MXB Clean / MXB Photo, one click each.
 *
 * Choosing a preset from Off downloads the official ReShade from reshade.me and a handful of
 * light effects, each checksum-pinned, and installs them into the game folder; Off takes
 * them away and puts back whatever was there. The player's own ReShade is never replaced
 * without asking. The backend is `reshade_pack`.
 */
export default function GraphicsPresetCard() {
  const t = useT();
  const { config } = useConfig();
  const { running } = useGameRunning();
  const [status, setStatus] = useState<GraphicsPresetStatus | null>(null);
  const [busy, setBusy] = useState<GraphicsPreset | null>(null);
  const [confirm, setConfirm] = useState<{ preset: GraphicsPreset; existing: GraphicsExisting } | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await graphicsPresetStatus());
    } catch {
      setStatus(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh, config.gamePath, config.reshadePath]);

  const choose = async (preset: GraphicsPreset, replaceExisting = false) => {
    setBusy(preset);
    try {
      const out = await setGraphicsPreset(preset, replaceExisting);
      setStatus(out.status);
      if (out.needsConfirm) {
        setConfirm({ preset, existing: out.needsConfirm });
        return;
      }
      if (preset === "off") {
        toast.success(t("graphics.offDone"));
      } else {
        toast.success(
          out.gameRunning
            ? t("graphics.onNextLaunch", { name: LABEL[preset] })
            : t("graphics.onDone", { name: LABEL[preset] }),
        );
      }
    } catch (e) {
      toast.error(t("graphics.failed"), { description: String(e) });
      void refresh();
    } finally {
      setBusy(null);
    }
  };

  const [converting, setConverting] = useState(false);
  const convertHires = async () => {
    setConverting(true);
    try {
      const out = await convertHiresShots();
      if (out.converted > 0) toast.success(t("graphics.convertedHires", { count: out.converted }));
      if (out.failed.length > 0) {
        toast.error(t("graphics.convertHiresFailed", { names: out.failed.join(", ") }));
      }
    } catch (e) {
      toast.error(t("graphics.convertHiresFailed", { names: String(e) }));
    } finally {
      setConverting(false);
      void refresh();
    }
  };

  return (
    <>
      <GraphicsPresetView
        status={status}
        busy={busy}
        running={running}
        onChoose={(p) => void choose(p)}
        converting={converting}
        onOpenPhotos={(hires) => {
          openPhotoFolder(hires).catch((e) =>
            toast.error(t("graphics.openPhotosFailed"), { description: String(e) }),
          );
        }}
        onConvertHires={() => void convertHires()}
      />
      <AlertDialog open={confirm !== null} onOpenChange={(o) => !o && setConfirm(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("graphics.replaceTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {confirm &&
                t("graphics.replaceBody", { what: describeExisting(confirm.existing, t) })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                const c = confirm;
                setConfirm(null);
                if (c) void choose(c.preset, true);
              }}
            >
              {t("graphics.replace")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

/** Everything the card shows, from props alone — so it renders headless in tests. */
export function GraphicsPresetView({
  status,
  busy,
  running,
  onChoose,
  converting = false,
  onOpenPhotos = () => {},
  onConvertHires = () => {},
}: {
  status: GraphicsPresetStatus | null;
  busy: GraphicsPreset | null;
  running: boolean;
  onChoose: (p: GraphicsPreset) => void;
  converting?: boolean;
  /** `true` opens MX Bikes' own F10 folder instead of the photo-mode one. */
  onOpenPhotos?: (hires: boolean) => void;
  onConvertHires?: () => void;
}) {
  const t = useT();

  if (!status || !status.gameFound) {
    return (
      <p className="text-[12px] leading-relaxed text-muted-foreground">
        {t("graphics.needsGameFolder")}
      </p>
    );
  }

  const current: GraphicsPreset | "" = status.choice ?? "";
  const shown: GraphicsPreset = busy ?? (current || "clean");

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-6">
        <span className="text-[12.5px] text-foreground/85">{t("graphics.label")}</span>
        <Segmented<GraphicsPreset | "">
          size="sm"
          value={busy ?? current}
          onChange={(v) => {
            if (busy === null && v && v !== current) onChoose(v);
          }}
          options={[
            { value: "off", label: t("graphics.off") },
            { value: "clean", label: LABEL.clean },
            { value: "photo", label: LABEL.photo },
          ]}
        />
      </div>

      <p className="flex items-center gap-1.5 text-[11.5px] leading-relaxed text-muted-foreground">
        {busy !== null && <LoaderCircle className="size-3.5 shrink-0 animate-spin" />}
        {busy !== null
          ? busy === "off"
            ? t("graphics.restoring")
            : status.installed
              ? t("graphics.switching")
              : t("graphics.downloading", { version: status.reshadeVersion })
          : status.choice === null
            ? t("graphics.customPreset")
            : t(DESC[shown])}
      </p>

      {status.installed && status.choice !== "off" && busy === null && (
        <PhotoMode
          status={status}
          converting={converting}
          onOpenPhotos={onOpenPhotos}
          onConvertHires={onConvertHires}
        />
      )}

      {!status.installed && status.existing && (
        <Notice tone="warning">
          {t("graphics.existingWarning", { what: describeExisting(status.existing, t) })}
        </Notice>
      )}

      {status.installed && status.disabled && (
        <Notice tone="info">{t("graphics.turnedOff")}</Notice>
      )}

      {running && (
        <Notice tone="info">{t("graphics.closeGame")}</Notice>
      )}

      <p className="text-[11px] leading-relaxed text-muted-foreground">
        {t("graphics.credits", { version: status.reshadeVersion })}
      </p>
    </div>
  );
}

/** MX Bikes' own high-res screenshot key (size from core.ini `[hrs]`, saved to hrsshots\). */
const GAME_HIRES_KEY = "F10";

const SUPER_RES: Record<"nvidia" | "amd" | "other", TKey> = {
  nvidia: "graphics.superResNvidia",
  amd: "graphics.superResAmd",
  other: "graphics.superResOther",
};

/**
 * The in-game keys for taking shots, read back from ReShade's own files so a key the player
 * rebinds in ReShade's menu shows here as they set it.
 */
function PhotoMode({
  status,
  converting,
  onOpenPhotos,
  onConvertHires,
}: {
  status: GraphicsPresetStatus;
  converting: boolean;
  onOpenPhotos: (hires: boolean) => void;
  onConvertHires: () => void;
}) {
  const t = useT();
  const k = status.hotkeys;
  const rows: { key: string; label: TKey }[] = [
    { key: k.photo, label: "graphics.keyPhoto" },
    { key: k.dof, label: "graphics.keyDof" },
    { key: k.screenshot, label: "graphics.keyScreenshot" },
    { key: GAME_HIRES_KEY, label: "graphics.keyHiRes" },
    { key: k.overlay, label: "graphics.keyOverlay" },
  ];
  const vendor = status.gpu === "nvidia" || status.gpu === "amd" ? status.gpu : "other";

  return (
    <div className="flex flex-col gap-2.5 rounded-lg border border-border bg-secondary/40 p-3">
      <span className="flex items-center gap-1.5 text-[12px] font-medium text-foreground/85">
        <Camera className="size-3.5 text-muted-foreground" />
        {t("graphics.photoMode")}
      </span>
      <ul className="flex flex-col gap-1.5">
        {rows
          .filter((r) => r.key !== "")
          .map((r) => (
            <li key={r.label} className="flex items-center gap-2.5 text-[12px] text-foreground/85">
              <kbd className="min-w-[2.75rem] rounded border border-border bg-secondary/60 px-1.5 py-px text-center font-mono text-[11px]">
                {r.key}
              </kbd>
              <span>{t(r.label)}</span>
            </li>
          ))}
      </ul>
      <p className="text-[11.5px] leading-relaxed text-muted-foreground">{t("graphics.photoTips")}</p>
      <p className="text-[11.5px] leading-relaxed text-muted-foreground">{t(SUPER_RES[vendor])}</p>
      <div className="flex min-w-0 items-center gap-2.5">
        <Button size="sm" variant="outline" onClick={() => onOpenPhotos(false)}>
          <FolderOpen className="size-3.5" />
          {t("graphics.openPhotos")}
        </Button>
        <span className="min-w-0 truncate text-[11px] text-muted-foreground" title={status.photoFolder}>
          {status.photoFolder}
        </span>
      </div>
      {status.hiresFolder !== "" && (
        <div className="flex flex-wrap items-center gap-2.5">
          <Button size="sm" variant="outline" onClick={() => onOpenPhotos(true)}>
            <FolderOpen className="size-3.5" />
            {t("graphics.openHires")}
          </Button>
          {status.hiresPending > 0 && (
            <Button size="sm" variant="outline" disabled={converting} onClick={onConvertHires}>
              {converting ? (
                <LoaderCircle className="size-3.5 animate-spin" />
              ) : (
                <ImageDown className="size-3.5" />
              )}
              {t("graphics.convertHires", { count: status.hiresPending })}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

function Notice({ tone, children }: { tone: "warning" | "info"; children: React.ReactNode }) {
  const Icon = tone === "warning" ? TriangleAlert : Info;
  return (
    <div
      className={
        tone === "warning"
          ? "flex gap-2 rounded-lg border border-warning/40 bg-warning/10 p-3"
          : "flex gap-2 rounded-lg border border-border bg-secondary/40 p-3"
      }
    >
      <Icon
        className={
          tone === "warning"
            ? "mt-px size-4 shrink-0 text-warning"
            : "mt-px size-4 shrink-0 text-muted-foreground"
        }
      />
      <p className="text-[12px] leading-relaxed text-foreground/85">{children}</p>
    </div>
  );
}
