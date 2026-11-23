import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { useT } from "@/i18n";

interface Fix {
  id: string;
  name: string;
  on: boolean;
  session: number;
  total: number;
  unit: string;
}

interface Status {
  isServerFolder: boolean;
  isGameFolder: boolean;
  installed: boolean;
  version: string | null;
  latest: string | null;
  latestError: string | null;
  protection: { fixes: Fix[] } | null;
}

const KEY = "mxb.frostserverFolder";

function remembered(): string {
  try {
    return localStorage.getItem(KEY) ?? "";
  } catch {
    return "";
  }
}

/**
 * FrostServer for a dedicated server this player hosts: pick its folder once, then install or
 * update in one click, and see what it has done on that server.
 */
export default function HostFrostServer() {
  const t = useT();
  const [folder, setFolder] = useState(remembered);
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);

  const load = useCallback(async () => {
    if (!folder) { setStatus(null); return; }
    setStatus(await invoke<Status>("frostserver_local_status", { folder }));
  }, [folder]);
  useEffect(() => { void load(); }, [load]);

  const choose = async () => {
    const picked = await pickFolder({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    setNote(null);
    setFolder(picked);
    try { localStorage.setItem(KEY, picked); } catch { /* per-viewer convenience only */ }
  };

  const install = async () => {
    setBusy(true); setNote(null);
    try {
      const v = await invoke<string>("frostserver_local_install", { folder });
      setNote({ ok: true, text: t("protection.hostDone", { v }) });
    } catch (e) {
      setNote({ ok: false, text: String(e) });
    }
    setBusy(false);
    void load();
  };

  const latest = status?.latest ?? null;
  const behind = !!status?.installed && !!latest && (status.version ?? "") !== latest;
  const blocked = !status || !status.isServerFolder || status.isGameFolder || !latest;
  const label = !status?.installed
    ? t("protection.hostInstall", { v: latest ?? "" })
    : behind
      ? t("protection.hostUpdate", { v: latest ?? "" })
      : t("protection.hostReinstall");

  return (
    <div className="flex flex-col">
      <div className="border-b border-border pb-1.5 text-[13px] font-semibold">{t("protection.host")}</div>
      <p className="py-2 text-[12px] leading-relaxed text-muted-foreground">{t("protection.hostDesc")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-muted-foreground">{folder || "—"}</span>
        <button type="button" onClick={() => void choose()} className="h-7 border border-input px-2.5 text-[12px] hover:bg-card">
          {folder ? t("protection.hostChange") : t("protection.hostPick")}
        </button>
        {folder && (
          <button
            type="button"
            disabled={busy || blocked}
            onClick={() => void install()}
            className="h-7 bg-primary px-3 text-[12px] font-medium text-primary-foreground disabled:opacity-50"
          >
            {label}
          </button>
        )}
      </div>
      {status && (
        <p className="pt-1.5 text-[11.5px] text-muted-foreground">
          {status.isGameFolder
            ? t("protection.hostIsGame")
            : !status.isServerFolder
              ? t("protection.hostNotServer")
              : status.installed
                ? t("protection.hostInstalled", { v: status.version || "?" })
                : t("protection.hostNotInstalled")}
          {status.latestError ? ` · ${status.latestError}` : ""}
        </p>
      )}
      {note && <p className={`pt-1.5 text-[12px] ${note.ok ? "text-success" : "text-destructive"}`}>{note.text}</p>}
      {status?.protection?.fixes && (
        <ul className="pt-2">
          {status.protection.fixes.map((f) => (
            <li key={f.id} className="flex items-baseline gap-3 border-b border-border/60 py-1.5 text-[12px]">
              <span className="flex-1">{f.name}</span>
              <span className="tabular-nums">{f.total.toLocaleString()}</span>
              <span className="w-[150px] text-muted-foreground">{f.on ? f.unit : "—"}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
