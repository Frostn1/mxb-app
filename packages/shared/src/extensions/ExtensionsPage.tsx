import { useState } from "react";
import { toast } from "sonner";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Lock, RefreshCw, ShieldCheck } from "lucide-react";
import { Button } from "../Components/ui/button";
import { Input } from "../Components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../Components/ui/select";
import { Switch } from "../Components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../Components/ui/dialog";
import { useT, type BaseTKey } from "../i18n/context";
import { cn } from "../lib/utils";
import { colorwayUnlocked, useTheme } from "../Context/Theme";
import { hasLooks, useSupporter } from "../api/supporter";
import {
  extCheckUpdates,
  extInspectFile,
  extInspectUrl,
  extInstall,
  extSetAutoUpdate,
  extSetDeveloperMode,
  extSetEnabled,
  extSetPermissions,
  extUninstall,
} from "./api";
import { themeSwatch } from "./appearance";
import { BUILTIN_EXTENSIONS, BUILTIN_THEMES } from "./builtins";
import { callExtensionTool, clearExtensionData, extensionSettings, runCommand, setExtensionSetting } from "./bridge";
import { cleanLabel, list, type InspectedPackage, type InstalledExtension, type Trust } from "./manifest";
import { isPermission, SENSITIVE, type Permission } from "./permissions";
import { enabledExtensions, refreshExtensions, setPick, useExtensions, parsePick } from "./store";
import { TrustLabel, trustColor } from "./trust";

export const PERM_LABEL: Record<Permission, BaseTKey> = {
  "mods.read": "ext.perm.mods",
  "servers.read": "ext.perm.servers",
  "friends.presence": "ext.perm.friends",
  "game.state": "ext.perm.game",
  "presets.read": "ext.perm.presets",
  "coach.read": "ext.perm.coach",
  "studio.launch": "ext.perm.studio",
  network: "ext.perm.network",
  openExternal: "ext.perm.openExternal",
  "mcp.tools": "ext.perm.mcp",
};

export function TrustBadge({ trust }: { trust: Trust }) {
  return <TrustLabel trust={trust} />;
}

function NewTag() {
  const t = useT();
  return (
    <span className="ml-1 rounded border px-1 py-px text-[10px] font-medium" style={{ color: trustColor("warning"), borderColor: `${trustColor("warning")}66` }}>
      {t("ext.new")}
    </span>
  );
}

function contributionSummary(m: InstalledExtension["manifest"], t: ReturnType<typeof useT<BaseTKey>>): string {
  const parts: string[] = [];
  const n = (k: Parameters<typeof list>[1]) => list(m, k).length;
  if (n("themes")) parts.push(t("ext.c.themes", { n: n("themes") }));
  if (n("buttonStyles")) parts.push(t("ext.c.buttons", { n: n("buttonStyles") }));
  if (n("backgrounds")) parts.push(t("ext.c.backgrounds", { n: n("backgrounds") }));
  if (n("fonts")) parts.push(t("ext.c.fonts", { n: n("fonts") }));
  if (n("views")) parts.push(t("ext.c.views", { n: n("views") }));
  if (n("commands")) parts.push(t("ext.c.commands", { n: n("commands") }));
  const tools = m.contributes?.mcp?.tools;
  if (Array.isArray(tools) && tools.length) parts.push(t("ext.c.tools", { n: tools.length }));
  return parts.join("  ·  ");
}

/** The consent screen. Nothing is on disk until Install is pressed here. */
export function InstallDialog({
  pkg,
  onClose,
  onInstalled,
}: {
  pkg: InspectedPackage;
  onClose: () => void;
  /** After a successful install, with the extension's id. */
  onInstalled?: (id: string) => void;
}) {
  const t = useT();
  const asked = (pkg.manifest.permissions ?? []).filter(isPermission);
  const s = useExtensions();
  // Reading other people's data starts unticked: it has to be a choice, not a default. An
  // update keeps what the person already chose for the installed version.
  const [granted, setGranted] = useState<Set<string>>(() => {
    const prior = pkg.replaces ? s.installed.find((e) => e.id === pkg.manifest.id) : undefined;
    return new Set(
      asked.filter((p) => {
        if (pkg.newPermissions.includes(p)) return false;
        return prior && (prior.manifest.permissions ?? []).includes(p) ? prior.granted.includes(p) : !SENSITIVE.includes(p);
      }),
    );
  });
  const [busy, setBusy] = useState(false);
  const m = pkg.manifest;

  const install = async () => {
    setBusy(true);
    try {
      const id = await extInstall(pkg.token, [...granted]);
      await refreshExtensions();
      onInstalled?.(id);
      toast.success(t("ext.installed", { name: cleanLabel(m.name, 60) }));
      onClose();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {pkg.replaces ? t("ext.updateTitle", { name: cleanLabel(m.name, 60) }) : t("ext.installTitle", { name: cleanLabel(m.name, 60) })}
          </DialogTitle>
          <DialogDescription>
            {t("ext.byline", { version: m.version, author: cleanLabel(m.author, 60) })}
            {pkg.replaces ? ` · ${t("ext.replaces", { version: pkg.replaces })}` : ""}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 text-[12.5px]">
          {pkg.trust === "verified" ? (
            <p className="inline-flex items-center gap-1" style={{ color: trustColor("verified") }}>
              <ShieldCheck className="size-3.5" />
              {t("ext.installVerified", { publisher: cleanLabel(pkg.publisher ?? m.author, 60) })}
            </p>
          ) : (
            <div
              className="space-y-1 rounded-md border px-3 py-2"
              style={{ color: trustColor(pkg.trust === "unsigned" ? "danger" : "warning"), borderColor: `${trustColor("warning")}66` }}
            >
              {pkg.source === "catalog" ? (
                pkg.trust === "signed" && <p>{t("ext.warn.catalogSigned", { author: cleanLabel(m.author, 60) })}</p>
              ) : (
                <>
                  <p>{t("ext.warn.sideload")}</p>
                  {pkg.trust === "signed" && <p>{t("ext.warn.sideloadClaim", { author: cleanLabel(m.author, 60) })}</p>}
                </>
              )}
              {pkg.trust === "unsigned" && <p>{t("ext.warn.unsigned")}</p>}
            </div>
          )}
          {m.description && <p className="text-muted-foreground">{cleanLabel(m.description, 400)}</p>}
          <p className="text-muted-foreground">{contributionSummary(m as InstalledExtension["manifest"], t) || t("ext.c.none")}</p>

          <div>
            <div className="mb-1.5 font-medium text-foreground/90">{t("ext.can")}</div>
            {(pkg.newPermissions.length > 0 || pkg.newHosts.length > 0) && (
              <p className="mb-2" style={{ color: trustColor("warning") }}>{t("ext.moreAccess")}</p>
            )}
            {asked.length === 0 ? (
              <p className="text-muted-foreground">{t("ext.noPermissions")}</p>
            ) : (
              <ul className="space-y-1.5">
                {asked.map((p) => (
                  <li key={p} className="flex items-start gap-2">
                    <input
                      type="checkbox"
                      className="mt-0.5 accent-primary"
                      checked={granted.has(p)}
                      onChange={(e) => {
                        const next = new Set(granted);
                        if (e.target.checked) next.add(p);
                        else next.delete(p);
                        setGranted(next);
                      }}
                    />
                    <span>
                      {t(p === "network" && !m.network?.length ? "ext.perm.networkNoHosts" : PERM_LABEL[p])}
                      {pkg.newPermissions.includes(p) && <NewTag />}
                      {p === "network" && m.network?.length ? (
                        <span className="block text-[11.5px] text-muted-foreground">
                          {m.network.map((host) => (
                            <span key={host} className="mr-2 inline-block">
                              {host}{pkg.newHosts.includes(host) && <NewTag />}
                            </span>
                          ))}
                        </span>
                      ) : null}
                    </span>
                  </li>
                ))}
              </ul>
            )}
            {asked.length > 0 && <p className="mt-2 text-[11.5px] text-muted-foreground">{t("ext.dataLeaves")}</p>}
          </div>
          {pkg.blocked && <p style={{ color: trustColor("danger") }}>{pkg.blocked}</p>}
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            {t("ext.cancel")}
          </Button>
          <Button disabled={busy || Boolean(pkg.blocked)} onClick={() => void install()}>
            {pkg.trust === "verified"
              ? pkg.replaces ? t("ext.update") : t("ext.install")
              : pkg.replaces ? t("ext.updateAnyway") : t("ext.installAnyway")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SettingField({ ext, def, value }: { ext: InstalledExtension; def: NonNullable<NonNullable<InstalledExtension["manifest"]["contributes"]>["settings"]>[number]; value: unknown }) {
  const label = cleanLabel(def.label, 60);
  const set = (v: unknown) => {
    setExtensionSetting(ext, def.key, v);
    void refreshExtensions();
  };
  return (
    <label className="flex items-center justify-between gap-3 text-[12.5px]">
      <span className="text-foreground/85">{label}</span>
      {def.type === "boolean" ? (
        <Switch checked={value === true} onCheckedChange={set} />
      ) : def.type === "enum" ? (
        <select
          className="h-8 rounded-md border border-input bg-transparent px-2"
          value={String(value ?? "")}
          onChange={(e) => set(e.target.value)}
        >
          {(def.options ?? []).map((o) => (
            <option key={o} value={o}>
              {cleanLabel(o, 40)}
            </option>
          ))}
        </select>
      ) : (
        <Input
          className="h-8 w-48"
          type={def.type === "number" ? "number" : "text"}
          defaultValue={value == null ? "" : String(value)}
          onBlur={(e) => set(def.type === "number" ? Number(e.target.value) : e.target.value.slice(0, 500))}
        />
      )}
    </label>
  );
}

function ExtensionCard({ ext, developerMode, onUpdate }: { ext: InstalledExtension; developerMode: boolean; onUpdate: (url: string) => void }) {
  const t = useT();
  const m = ext.manifest;
  const builtin = ext.trust === "builtin";
  const asked = (m.permissions ?? []).filter(isPermission);
  const commands = list(m, "commands");
  const settings = list(m, "settings");
  const values = extensionSettings(ext);
  const running = ext.enabled && !ext.suspended;
  const act = (p: Promise<unknown>) =>
    void p.then(() => refreshExtensions()).catch((e) => toast.error(String(e)));

  return (
    <div className="border-b border-border py-4 last:border-b-0">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex flex-wrap items-baseline gap-x-2">
            <span className="text-[13.5px] font-semibold text-foreground">{cleanLabel(m.name, 60)}</span>
            <span className="text-[11.5px] text-muted-foreground">
              {t("ext.byline", { version: ext.version, author: cleanLabel(m.author, 60) })}
            </span>
          </div>
          <TrustBadge trust={ext.trust} />
          {m.description && <p className="mt-1 text-[12.5px] text-muted-foreground">{cleanLabel(m.description, 400)}</p>}
          <p className="mt-1 text-[11.5px] text-faint">{contributionSummary(m, t)}</p>
        </div>
        {!builtin && (
          <Switch checked={running} disabled={ext.suspended} onCheckedChange={(on) => act(extSetEnabled(ext.id, on))} />
        )}
      </div>

      {ext.suspended && (
        <p className="mt-2 text-[12.5px]" style={{ color: trustColor("warning") }}>
          {t("ext.pausedUnsigned")}
        </p>
      )}

      {ext.pendingUpdate && (
        <div className="mt-2 flex items-center justify-between gap-3 text-[12.5px]">
          <span>
            {t("ext.pending", {
              version: ext.pendingUpdate.version,
              details: [
                ...ext.pendingUpdate.newPermissions.filter(isPermission).map((p) => t(PERM_LABEL[p])),
                ...ext.pendingUpdate.newHosts.map((host) => t("ext.hostAccess", { host })),
              ].join("; "),
            })}
          </span>
          <Button size="sm" onClick={() => onUpdate(ext.pendingUpdate!.url)}>
            {t("ext.review")}
          </Button>
        </div>
      )}

      {!builtin && asked.length > 0 && (
        <div className="mt-3 space-y-1">
          {asked.map((p) => (
            <label key={p} className="flex items-start gap-2 text-[12px] text-foreground/85">
              <input
                type="checkbox"
                className="mt-0.5 accent-primary"
                checked={ext.granted.includes(p)}
                onChange={(e) =>
                  act(
                    extSetPermissions(
                      ext.id,
                      e.target.checked ? [...ext.granted, p] : ext.granted.filter((g) => g !== p),
                    ),
                  )
                }
              />
              <span>
                {t(p === "network" && !m.network?.length ? "ext.perm.networkNoHosts" : PERM_LABEL[p])}
                {p === "network" && m.network?.length ? (
                  <span className="block pl-0 text-[11.5px] text-muted-foreground">{m.network.join(", ")}</span>
                ) : null}
              </span>
            </label>
          ))}
        </div>
      )}

      {running && settings.length > 0 && (
        <div className="mt-3 space-y-2">
          {settings
            .filter((s) => s && typeof s.key === "string")
            .map((s) => (
              <SettingField key={s.key} ext={ext} def={s} value={values[s.key]} />
            ))}
        </div>
      )}

      {!builtin && (
        <div className="mt-3 flex flex-wrap items-center gap-2">
          {running &&
            commands
              .filter((c) => c && typeof c.id === "string")
              .map((c) => (
                <Button
                  key={c.id}
                  size="sm"
                  variant="secondary"
                  onClick={() => {
                    if (!runCommand(ext.id, c.id)) toast(t("ext.notRunning", { name: cleanLabel(m.name, 60) }));
                  }}
                >
                  {cleanLabel(c.title, 40)}
                </Button>
              ))}
          {m.updateUrl && (
            <label className="flex items-center gap-2 text-[12px] text-muted-foreground">
              <Switch checked={ext.autoUpdate} onCheckedChange={(on) => act(extSetAutoUpdate(ext.id, on))} />
              {t("ext.autoUpdate")}
            </label>
          )}
          <div className="ml-auto flex gap-2">
            {m.updateUrl && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  void extCheckUpdates(ext.id)
                    .then(async (r) => {
                      const o = r[0];
                      if (o?.status === "failed") toast.error(o.message ?? "");
                      else if (o?.status === "current") toast(t("ext.upToDate"));
                      await refreshExtensions();
                    })
                    .catch((e) => toast.error(String(e)))
                }
              >
                {t("ext.checkUpdate")}
              </Button>
            )}
            <Button
              size="sm"
              variant="destructive"
              onClick={() => {
                clearExtensionData(ext.id);
                act(extUninstall(ext.id));
              }}
            >
              {t("ext.uninstall")}
            </Button>
          </div>
        </div>
      )}
      {developerMode && running && ext.granted.includes("mcp.tools") && (
        <div className="mt-2 flex flex-wrap gap-2">
          {(m.contributes?.mcp?.tools ?? [])
            .filter((tool) => typeof tool?.name === "string")
            .map((tool) => (
              <Button
                key={tool.name}
                size="sm"
                variant="ghost"
                onClick={() =>
                  void callExtensionTool(ext.id, tool.name, {})
                    .then((r) => toast(t("ext.toolResult", { name: tool.name }), { description: JSON.stringify(r).slice(0, 300) }))
                    .catch((e) => toast.error(String(e)))
                }
              >
                {t("ext.tryTool", { name: cleanLabel(tool.name, 40) })}
              </Button>
            ))}
        </div>
      )}
      {developerMode && !builtin && (
        <p className="mt-2 text-[11px] text-faint">
          {ext.id} · {ext.source}
          {ext.key ? ` · key ${ext.key}` : ""}
        </p>
      )}
    </div>
  );
}

/** Theme, button style and background pickers over every enabled extension and the built-ins. */
export function AppearancePickers({ builtins = true }: { builtins?: boolean }) {
  const t = useT();
  const s = useExtensions();
  const { colorway, setColorway } = useTheme();
  const looks = hasLooks(useSupporter());
  const enabled = enabledExtensions(s);
  const extThemes = enabled.flatMap((e) =>
    list(e.manifest, "themes")
      .filter((th) => th && typeof th.id === "string")
      .map((th) => ({ key: `${e.id}/${th.id}`, label: cleanLabel(th.label), from: cleanLabel(e.manifest.name, 60), swatch: themeSwatch(th) })),
  );
  const buttons = enabled.flatMap((e) =>
    list(e.manifest, "buttonStyles").filter((b) => b && typeof b.id === "string").map((b) => ({ key: `${e.id}/${b.id}`, label: cleanLabel(b.label) })),
  );
  const backgrounds = enabled.flatMap((e) =>
    list(e.manifest, "backgrounds").filter((b) => b && typeof b.id === "string").map((b) => ({ key: `${e.id}/${b.id}`, label: cleanLabel(b.label) })),
  );
  const pickedTheme = parsePick(s.picks.theme) ? s.picks.theme : null;

  const swatch = (key: string, label: string, [tint, chrome]: [string, string], active: boolean, onClick: () => void, locked = false) => (
    <button
      key={key}
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={active}
      disabled={locked}
      onClick={onClick}
      className={cn(
        "size-6 cursor-default rounded-full border transition-shadow disabled:opacity-40",
        active ? "border-primary ring-2 ring-primary/40" : "border-input hover:border-foreground/30",
      )}
      style={{ background: `linear-gradient(135deg, ${tint} 0 50%, ${chrome} 50% 100%)` }}
    />
  );

  return (
    <div className="space-y-3">
      <div className="flex items-start justify-between gap-4">
        {/* Beside Settings' own Theme (light/dark) row, this one needs its own name. */}
        <span className="text-[12.5px] text-foreground/85">{t(builtins ? "ext.theme" : "ext.extThemes")}</span>
        <div className="flex max-w-[420px] flex-wrap justify-end gap-1.5">
          {(builtins ? BUILTIN_THEMES : []).map((b) =>
            swatch(
              b.id,
              b.label,
              b.swatch,
              !pickedTheme && colorway === b.id,
              () => {
                setPick("theme", null);
                setColorway(b.id);
              },
              b.pack && !colorwayUnlocked(b.id, looks),
            ),
          )}
          {extThemes.map((x) =>
            swatch(x.key, `${x.label} (${x.from})`, x.swatch, pickedTheme === x.key, () => setPick("theme", x.key)),
          )}
        </div>
      </div>
      {builtins && !looks && (
        <p className="flex items-center justify-end gap-1 text-[11px] text-muted-foreground">
          <Lock className="size-3" />
          {t("ext.packLocked")}
        </p>
      )}
      <div className="flex items-center justify-between gap-4">
        <span className="text-[12.5px] text-foreground/85">{t("ext.buttons")}</span>
        <Select
          value={s.picks.buttons ?? "__none"}
          onValueChange={(value) => setPick("buttons", value === "__none" ? null : value)}
        >
          <SelectTrigger className="h-8 w-[180px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__none">{t("ext.default")}</SelectItem>
            {buttons.map((b) => (
              <SelectItem key={b.key} value={b.key}>
                {b.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="flex items-center justify-between gap-4">
        <span className="text-[12.5px] text-foreground/85">{t("ext.background")}</span>
        <Select
          value={s.picks.background ?? "__none"}
          onValueChange={(value) => setPick("background", value === "__none" ? null : value)}
        >
          <SelectTrigger className="h-8 w-[180px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__none">{t("ext.none")}</SelectItem>
            {backgrounds.map((b) => (
              <SelectItem key={b.key} value={b.key}>
                {b.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </div>
  );
}

/**
 * Extensions → Installed: install, consent, enable, configure, update and remove. Shared, so
 * any app in the lineup that mounts `ExtensionAppearance` and `ExtensionRuntime` gets the same page.
 */
/**
 * What is installed, and the ways in besides the marketplace: a file, a URL or (developer
 * mode) a folder. The marketplace's Installed view.
 */
export default function ExtensionsPage({ showLooks = true }: { showLooks?: boolean }) {
  const t = useT();
  const s = useExtensions();
  const [url, setUrl] = useState("");
  const [pkg, setPkg] = useState<InspectedPackage | null>(null);
  const [busy, setBusy] = useState(false);

  const inspect = async (p: Promise<InspectedPackage>) => {
    setBusy(true);
    try {
      setPkg(await p);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const pickFile = async () => {
    const path = await openDialog({ multiple: false, filters: [{ name: "MXB extension", extensions: ["mxbext"] }] });
    if (typeof path === "string") await inspect(extInspectFile(path));
  };
  const pickFolder = async () => {
    const path = await openDialog({ directory: true, multiple: false });
    if (typeof path === "string") await inspect(extInspectFile(path));
  };

  const all = [...BUILTIN_EXTENSIONS, ...s.installed];

  return (
    <div className="space-y-6">
      {showLooks && <AppearancePickers />}

      <div className="space-y-2 border-t border-border pt-4">
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" disabled={busy} onClick={() => void pickFile()}>
            {t("ext.fromFile")}
          </Button>
          {s.developerMode && (
            <Button size="sm" variant="secondary" disabled={busy} onClick={() => void pickFolder()}>
              {t("ext.fromFolder")}
            </Button>
          )}
          <form
            className="flex min-w-[260px] flex-1 items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (url.trim()) void inspect(extInspectUrl(url.trim()));
            }}
          >
            <Input className="h-8" placeholder={t("ext.urlPlaceholder")} value={url} onChange={(e) => setUrl(e.target.value)} />
            <Button size="sm" variant="secondary" type="submit" disabled={busy || !url.trim().startsWith("https://")}>
              {t("ext.fromUrl")}
            </Button>
          </form>
          <Button
            size="sm"
            variant="ghost"
            onClick={() =>
              void extCheckUpdates()
                .then(() => refreshExtensions())
                .then(() => toast(t("ext.checked")))
                .catch((e) => toast.error(String(e)))
            }
          >
            <RefreshCw className="size-3.5" />
            {t("ext.checkUpdates")}
          </Button>
        </div>
        <label className="flex items-center gap-2 text-[12px] text-muted-foreground">
          <Switch
            checked={s.developerMode}
            onCheckedChange={(on) => void extSetDeveloperMode(on).then(() => refreshExtensions())}
          />
          {t("ext.developerMode")}
        </label>
        {s.developerMode && <p className="text-[11.5px] text-warning">{t("ext.developerModeHint")}</p>}
      </div>

      <div className="border-t border-border">
        {all.map((e) => (
          <ExtensionCard
            key={e.id}
            ext={e}
            developerMode={s.developerMode}
            onUpdate={(u) => void inspect(extInspectUrl(u))}
          />
        ))}
        {s.error && <p className="py-3 text-[12px] text-destructive">{s.error}</p>}
      </div>

      {pkg && <InstallDialog pkg={pkg} onClose={() => setPkg(null)} />}
    </div>
  );
}
