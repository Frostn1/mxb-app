/**
 * The live share codes this machine publishes and follows.
 *
 * A `MXBS1-` code is the share written down, so a track author who recompiles has to send a
 * new one round. A live code is a permanent pointer the control plane repoints: send it
 * once, publish as many versions as you like. This is where both sides of that live — the
 * codes you own, with the button that pushes a new version, and the ones you follow, with
 * the version you have against the version that's out.
 *
 * Nothing here signs in. See `src-tauri/src/liveshare.rs`.
 */
import { useCallback, useEffect, useState } from "react";
import {
  Check,
  Cloud,
  Copy,
  Download,
  KeyRound,
  Loader2,
  MoreHorizontal,
  RefreshCw,
  Trash2,
  UploadCloud,
} from "lucide-react";
import { toast } from "sonner";
import {
  liveShareAdopt,
  liveShareCheck,
  liveShareForget,
  liveShareList,
  liveShareOwnerCode,
  liveSharePublish,
  liveShareSetAuto,
  liveShareSync,
  onFileShareProgress,
} from "../../api/mods";
import type { LiveShareInfo } from "../../types";
import { formatBytes } from "../../lib/mods";
import { copyText } from "../../lib/clipboard";
import { useT, type TFunc } from "../../i18n/context";
import { Button } from "@/Components/ui/button";
import { Switch } from "@/Components/ui/switch";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/Components/ui/dropdown-menu";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/Components/ui/dialog";
import { cn } from "@/lib/utils";

/** "2h ago", from unix seconds. `0` means it never happened. */
function ago(seconds: number, t: TFunc): string {
  if (!seconds) return t("live.never");
  const mins = Math.max(0, Math.round((Date.now() / 1000 - seconds) / 60));
  if (mins < 1) return t("live.justNow");
  if (mins < 60) return t("live.minsAgo", { count: mins });
  const hours = Math.round(mins / 60);
  if (hours < 24) return t("live.hoursAgo", { count: hours });
  return t("live.daysAgo", { count: Math.round(hours / 24) });
}

/**
 * One code, in three lines: what it is, where it stands, what you can do about it.
 *
 * The everyday actions are buttons; the rare and the dangerous ones live behind the menu.
 * Copying an owner key hands over the ability to replace the track for everyone following
 * the code, so it is deliberately not a button sitting next to the code you hand out.
 */
function Row({
  row,
  busy,
  onUpdate,
  onPublish,
  onAuto,
  onForget,
}: {
  row: LiveShareInfo;
  busy: string | null;
  onUpdate: (code: string) => void;
  onPublish: (row: LiveShareInfo) => void;
  onAuto: (code: string, auto: boolean) => void;
  onForget: (code: string) => void;
}) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const behind = !row.mine && row.latest > row.version;
  const working = busy === row.code;

  const facts = [
    behind
      ? t("live.versionBehind", { have: row.version, latest: row.latest })
      : t("live.versionAt", { version: row.version }),
    ...(row.size > 0 ? [formatBytes(row.size)] : []),
    row.mine
      ? t("live.publishedAgo", { when: ago(row.publishedAt, t) })
      : t("live.checkedAgo", { when: ago(Math.round(row.checkedAt / 1000), t) }),
  ];

  const copy = useCallback(
    async (text: string, message: string) => {
      if (await copyText(text)) {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
        toast.success(message);
      } else {
        toast.error(t("presets.copyFailed"));
      }
    },
    [t],
  );

  return (
    <div
      className={cn(
        "border-b border-white/[0.06] px-3 py-3 last:border-b-0",
        behind && "bg-sky-500/[0.06]",
      )}
    >
      <div className="flex items-center gap-2">
        <span className="truncate text-[13px] font-semibold">{row.name}</span>
        {row.mine ? (
          <span className="flex-none rounded bg-emerald-500/15 px-1.5 py-px text-[10px] font-semibold uppercase tracking-wide text-emerald-500">
            {t("live.yours")}
          </span>
        ) : behind ? (
          <span className="flex-none rounded bg-sky-500/15 px-1.5 py-px text-[10px] font-semibold uppercase tracking-wide text-sky-400">
            {t("live.updateReady")}
          </span>
        ) : null}
      </div>

      {/* One wrapping line rather than a column of half-empty rows — these are four short
          facts about the same thing, and stacking them made the list twice as tall. */}
      <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11.5px] text-muted-foreground">
        <span className="rounded bg-white/[0.06] px-1.5 py-px font-mono text-[11px] tracking-wide text-foreground/80">
          {row.code}
        </span>
        {/* Each fact keeps its own separator and never breaks inside itself, so a line that
            does wrap breaks between facts rather than stranding a lone dot. */}
        {facts.map((fact, i) => (
          <span key={fact} className="whitespace-nowrap">
            {i > 0 && <span className="mr-2">·</span>}
            {fact}
          </span>
        ))}
      </div>

      <div className="mt-2.5 flex items-center gap-1.5">
        <Button
          size="sm"
          variant="secondary"
          className="h-7 px-2 text-[11.5px]"
          onClick={() => void copy(row.code, t("live.codeCopied"))}
        >
          {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
          {t("live.copyCode")}
        </Button>

        {row.mine ? (
          <Button
            size="sm"
            className="h-7 px-2 text-[11.5px]"
            disabled={!!busy || row.rels.length === 0}
            onClick={() => onPublish(row)}
          >
            {working ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <UploadCloud className="size-3.5" />
            )}
            {t("live.publishUpdate")}
          </Button>
        ) : (
          <Button
            size="sm"
            className="h-7 px-2 text-[11.5px]"
            disabled={!!busy || !behind}
            onClick={() => onUpdate(row.code)}
          >
            {working ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Download className="size-3.5" />
            )}
            {behind ? t("live.update") : t("live.upToDate")}
          </Button>
        )}

        <div className="ml-auto flex items-center gap-2">
          {!row.mine && (
            <label className="flex cursor-pointer items-center gap-1.5 text-[11.5px] text-muted-foreground">
              <Switch
                checked={row.auto}
                onCheckedChange={(v) => onAuto(row.code, v)}
                disabled={!!busy}
              />
              {t("live.auto")}
            </label>
          )}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                size="sm"
                variant="ghost"
                className="size-7 p-0 text-muted-foreground"
                aria-label={t("live.moreActions")}
              >
                <MoreHorizontal className="size-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {row.mine && (
                <DropdownMenuItem
                  onSelect={async () => {
                    try {
                      await copy(await liveShareOwnerCode(row.code), t("live.ownerCopied"));
                    } catch (e) {
                      toast.error(String(e).replace(/^Error:\s*/, ""));
                    }
                  }}
                >
                  <KeyRound className="size-3.5" />
                  {t("live.ownerCode")}
                </DropdownMenuItem>
              )}
              <DropdownMenuItem
                onSelect={() => onForget(row.code)}
                className="text-destructive focus:text-destructive"
              >
                <Trash2 className="size-3.5" />
                {row.mine ? t("live.forgetMine") : t("live.unfollow")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </div>
  );
}

/** The live shares list, as a dialog off the Library's Import menu. */
export function LiveSharesDialog({
  open,
  onClose,
  onChanged,
}: {
  open: boolean;
  onClose: () => void;
  /** Fired after an update installs something, so library views can re-scan. */
  onChanged?: () => void;
}) {
  const t = useT();
  const [rows, setRows] = useState<LiveShareInfo[]>([]);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [adopt, setAdopt] = useState("");

  const load = useCallback(() => {
    liveShareList()
      .then(setRows)
      .catch((e) => toast.error(String(e).replace(/^Error:\s*/, "")));
  }, []);

  // Open on what is known locally, then ask the server once. A check is the only request
  // this screen makes on its own — see the note in `liveshare.rs` on the request budget.
  useEffect(() => {
    if (!open) return;
    load();
    setChecking(true);
    liveShareCheck()
      .then(setRows)
      .catch((e) => toast.error(String(e).replace(/^Error:\s*/, "")))
      .finally(() => setChecking(false));
  }, [open, load]);

  const check = useCallback(() => {
    setChecking(true);
    liveShareCheck()
      .then((r) => {
        setRows(r);
        const behind = r.filter((x) => !x.mine && x.latest > x.version).length;
        toast.success(behind ? t("live.foundUpdates", { count: behind }) : t("live.allCurrent"));
      })
      .catch((e) => toast.error(String(e).replace(/^Error:\s*/, "")))
      .finally(() => setChecking(false));
  }, [t]);

  const update = useCallback(
    async (code: string) => {
      setBusy(code);
      const unlisten = await onFileShareProgress(() => {});
      try {
        const done = await liveShareSync(code);
        toast.success(t("share.installed", { count: done.items.length }));
        onChanged?.();
        load();
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        unlisten();
        setBusy(null);
      }
    },
    [load, onChanged, t],
  );

  // Repacks the rels the code already carries, so publishing a rebuilt track is one click
  // rather than picking the same files out of the Library again.
  const publish = useCallback(
    async (row: LiveShareInfo) => {
      setBusy(row.code);
      const unlisten = await onFileShareProgress(() => {});
      try {
        const out = await liveSharePublish(row.rels, row.name, row.code);
        toast.success(t("live.published", { version: out.version }));
        load();
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        unlisten();
        setBusy(null);
      }
    },
    [load, t],
  );

  const setAuto = useCallback(
    (code: string, auto: boolean) => {
      liveShareSetAuto(code, auto)
        .then(load)
        .catch((e) => toast.error(String(e).replace(/^Error:\s*/, "")));
    },
    [load],
  );

  const forget = useCallback(
    (code: string) => {
      liveShareForget(code)
        .then(load)
        .catch((e) => toast.error(String(e).replace(/^Error:\s*/, "")));
    },
    [load],
  );

  const takeOver = useCallback(() => {
    const text = adopt.trim();
    if (!text) return;
    liveShareAdopt(text)
      .then((row) => {
        toast.success(t("live.adopted", { name: row.name }));
        setAdopt("");
        load();
      })
      .catch((e) => toast.error(String(e).replace(/^Error:\s*/, "")));
  }, [adopt, load, t]);

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-lg gap-4">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Cloud className="size-4" />
            {t("live.title")}
          </DialogTitle>
          <DialogDescription>{t("live.help")}</DialogDescription>
        </DialogHeader>

        {rows.length === 0 ? (
          <p className="py-8 text-center text-[12.5px] text-muted-foreground">{t("live.empty")}</p>
        ) : (
          <div className="max-h-[24rem] overflow-y-auto rounded-lg border border-white/[0.07] bg-card/40">
            {rows.map((row) => (
              <Row
                key={row.code}
                row={row}
                busy={busy}
                onUpdate={(c) => void update(c)}
                onPublish={(r) => void publish(r)}
                onAuto={setAuto}
                onForget={forget}
              />
            ))}
          </div>
        )}

        {/* Moving a share to another machine. Below a rule and under its own heading because
            it is the rare case — a reinstall, a second PC, handing a track over — and sitting
            it flush against the list made it look like part of the everyday flow. */}
        <div className="border-t border-white/[0.07] pt-3">
          <p className="text-[11.5px] font-semibold">{t("live.adoptTitle")}</p>
          <p className="mt-0.5 text-[11.5px] leading-snug text-muted-foreground">
            {t("live.adoptHint")}
          </p>
          <div className="mt-2 flex items-center gap-1.5">
            <input
              value={adopt}
              onChange={(e) => setAdopt(e.target.value)}
              placeholder={t("live.adoptPlaceholder")}
              className="min-w-0 flex-1 rounded-lg border border-input bg-transparent px-2.5 py-1.5 font-mono text-[11px] placeholder:text-faint focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            />
            <Button
              size="sm"
              variant="secondary"
              className="h-8 flex-none"
              disabled={!adopt.trim()}
              onClick={takeOver}
            >
              <KeyRound className="size-3.5" />
              {t("live.adopt")}
            </Button>
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            {t("common.close")}
          </Button>
          <Button variant="secondary" disabled={checking} onClick={check}>
            <RefreshCw className={cn("size-4", checking && "animate-spin")} />
            {checking ? t("live.checking") : t("live.check")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
