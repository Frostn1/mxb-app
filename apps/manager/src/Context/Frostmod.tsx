import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { toast } from "sonner";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import {
  frostmodClearStrayMsvcr90,
  frostmodInstall,
  frostmodInstallRuntime,
  frostmodRepairRuntimes,
  frostmodAttachment,
  frostmodStart,
  frostmodStatus,
  frostmodStop,
  isFrostmodRunning,
  MODS_WATCH_SLUG,
  onFrostmodReload,
  reloadFrostmod,
  RUNTIME_DOWNLOAD_URL,
  RUNTIME_DOWNLOADS_PAGE,
  RUNTIME_NAME_KEY,
  setAutoRunFrostmod,
} from "@frost/shared/api/mods";
import type { Attachment, FrostmodStatus, VcRuntime } from "@frost/shared/types";
import { ATTACH_PROBLEM } from "@frost/shared/types";
import { displayName } from "@frost/shared/lib/mods";
import { autoInstallAction } from "../lib/frostmodAuto";
import { recordActivity } from "../lib/activity";
import { useGameRunning } from "../lib/useGameRunning";
import { useT, type TFunc, type TKey } from "@/i18n";
import { FrostmodContext } from "./FrostmodContext";

const POLL_MS = 5000;
const INTEGRATION_CHOICE_KEY = "mxb.integration-choice.v1";

type IntegrationChoice = "enabled" | "app-only" | null;

function savedIntegrationChoice(): IntegrationChoice {
  try {
    const value = localStorage.getItem(INTEGRATION_CHOICE_KEY);
    return value === "enabled" || value === "app-only" ? value : null;
  } catch {
    return null;
  }
}

/**
 * How often to ask GitHub whether FrostMod has a newer release.
 *
 * The check used to run once, at mount, so an app left open never learned about a release
 * published after it started. Half-hourly is two calls an hour against GitHub's 60/hr
 * unauthenticated limit, and it also re-runs the sweeps `status()` carries — the stray
 * `msvcr90.dll` and the stale game plugin — which only ever ran that one time too.
 */
const VERSION_CHECK_MS = 30 * 60 * 1000;

/**
 * Name what the folder watcher picked up. The watcher reports mods as `<type>/<name>`;
 * showing them beats a generic "your folder changed", which left people unsure whether
 * the drop had been seen at all.
 *
 * Takes `t` rather than lowercasing a translated sentence to splice it mid-phrase —
 * that only works in languages that lowercase mid-sentence, which German doesn't.
 */
function watchDescription(mods: string[], t: TFunc<TKey>): string {
  if (mods.length === 0) return t("frostmod.askedReload");
  const names = mods.map((m) => displayName(m.split("/").pop() ?? m));
  const shown = names.slice(0, 3).join(", ");
  const rest = names.length - 3;
  const list =
    rest > 0 ? t("frostmod.andMore", { names: shown, count: rest }) : shown;
  return t("frostmod.watchDesc", { names: list });
}

export function FrostmodProvider({ children }: { children: ReactNode }) {
  const t = useT();
  // Only ever read to decide whether an unattended update may run now — FrostMod's own
  // process is `running` below, and the two are separate things to watch.
  const { running: gameRunning } = useGameRunning();
  const [running, setRunning] = useState<boolean | null>(null);
  const [integrationChoice, setIntegrationChoice] =
    useState<IntegrationChoice>(savedIntegrationChoice);
  const [integrationConsentOpen, setIntegrationConsentOpen] = useState(
    () => savedIntegrationChoice() === null,
  );
  const [attachment, setAttachment] = useState<Attachment | null>(null);
  // What we last warned about, so a problem that persists across the 5s poll is reported
  // once rather than every tick. Cleared when the state stops being a problem, which is
  // what lets the *next* game session warn again.
  const warnedFor = useRef<string | null>(null);
  const [status, setStatus] = useState<FrostmodStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  // React state does not update synchronously. This closes the small window where the
  // explicit Enable action and the compatibility effect could both start an install.
  const installFlight = useRef(false);
  const [checking, setChecking] = useState(false);
  const [statusError, setStatusError] = useState(false);
  const [installingRuntime, setInstallingRuntime] = useState(false);
  const [repairingRuntimes, setRepairingRuntimes] = useState(false);
  const [clearingStray, setClearingStray] = useState(false);
  const [runtimeDismissed, setRuntimeDismissed] = useState(false);
  const mounted = useRef(true);

  const rememberIntegrationChoice = useCallback((choice: Exclude<IntegrationChoice, null>) => {
    setIntegrationChoice(choice);
    setIntegrationConsentOpen(false);
    try {
      localStorage.setItem(INTEGRATION_CHOICE_KEY, choice);
    } catch {
      /* A locked-down webview still gets the choice for this session. */
    }
  }, []);

  const requestIntegrationConsent = useCallback(() => {
    setIntegrationConsentOpen(true);
  }, []);

  const probe = useCallback(async () => {
    try {
      const r = await isFrostmodRunning();
      if (mounted.current) setRunning(r);
    } catch {
      if (mounted.current) setRunning(false);
    }
    // Folded into the same tick rather than given a poll of its own: it answers the
    // follow-up to the question above, and asking them apart would let the pill show a
    // running FrostMod and a stale attach state at the same time.
    try {
      const a = await frostmodAttachment();
      if (!mounted.current) return;
      setAttachment(a);
      if (integrationChoice !== "enabled" || !ATTACH_PROBLEM.includes(a.state)) {
        warnedFor.current = null;
        return;
      }
      // The reason carries the game name, so a state that stays the same while the
      // player switches title still gets said once for each.
      const key = `${a.state}:${a.reason}`;
      if (warnedFor.current === key) return;
      warnedFor.current = key;
      toast.warning(t("frostmod.notInGame"), {
        description: a.reason,
        duration: 12000,
      });
    } catch {
      /* older backend or non-Tauri — leave the pill on `running` alone */
    }
  }, [integrationChoice, t]);

  const refreshStatus = useCallback(async () => {
    if (mounted.current) setChecking(true);
    try {
      const s = await frostmodStatus();
      if (mounted.current) {
        setStatus(s);
        setRunning(s.running);
        // A successful call always yields a `latest` tag; a null one means the
        // GitHub check inside `status()` failed even though the command returned.
        setStatusError(s.latest === null);
      }
    } catch {
      /* offline / non-Tauri — leave prior status but flag the failure */
      if (mounted.current) setStatusError(true);
    } finally {
      if (mounted.current) setChecking(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    probe();
    void refreshStatus();
    const id = setInterval(probe, POLL_MS);
    // Its own, much slower interval: this one hits the network, the one above doesn't.
    const versionId = setInterval(() => void refreshStatus(), VERSION_CHECK_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
      clearInterval(versionId);
    };
  }, [probe, refreshStatus]);

  // Surface reloads the mods-folder watcher triggers (a manual download dropped into
  // the folder). In-app installs carry their own slug and toast, so we only react to
  // the watcher's sentinel here.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onFrostmodReload((p) => {
      if (p.slug !== MODS_WATCH_SLUG) return;
      if (p.outcome === "signaled") {
        // "Added", not "loaded": signalling FrostMod only tells us its reload event
        // exists and was poked — whether the game picked the mods up is FrostMod's to
        // report, and claiming otherwise is what makes a failed reload so confusing.
        const mods = p.mods ?? [];
        toast.success(
          mods.length === 0
            ? t("frostmod.newModsAdded")
            : t("frostmod.modsAdded", { count: mods.length }),
          { description: watchDescription(mods, t) },
        );
      }
      void probe();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [probe, t]);

  const reload = useCallback(async () => {
    const outcome = await reloadFrostmod();
    probe();
    return outcome;
  }, [probe]);

  const start = useCallback(async () => {
    try {
      const started = await frostmodStart();
      await probe();
      if (started) toast.success(t("frostmod.started"));
      else toast.info(t("frostmod.alreadyRunning"));
    } catch (e) {
      toast.error(t("frostmod.startFailed"), { description: String(e) });
    }
  }, [probe, t]);

  const stop = useCallback(async () => {
    try {
      const stopped = await frostmodStop();
      await probe();
      // The backend waits for the process to actually go, so a `false` here is a
      // FrostMod that survived the kill — say so rather than let the pill contradict
      // a success toast a second later.
      if (stopped) toast.success(t("frostmod.stopped"));
      else
        toast.error(t("frostmod.stopFailed"), {
          description: t("frostmod.stopFailedDesc"),
        });
    } catch (e) {
      await probe();
      toast.error(t("frostmod.stopFailed"), { description: String(e) });
    }
  }, [probe, t]);

  const install = useCallback(async () => {
    if (installFlight.current) return;
    installFlight.current = true;
    // Pressing an Install/Repair button is itself explicit consent. Remember it so a
    // manually installed component can receive future unattended compatibility fixes.
    rememberIntegrationChoice("enabled");
    setInstalling(true);
    try {
      // The backend stops a running FrostMod before replacing it and restarts it
      // after — so we don't start it here, which would race that restart and spawn
      // a second instance.
      const { version, needsGameRestart } = await frostmodInstall();
      await refreshStatus();
      recordActivity({
        title: t("activity.integrationInstalled", { version }),
        status: "success",
      });
      toast.success(t("frostmod.installedToast", { version }), {
        // The update landed either way; when the game had the old FrostMod loaded,
        // it keeps running that until restarted, and saying so beats leaving people
        // to wonder why the new version isn't doing anything.
        description: needsGameRestart
          ? t("frostmod.installedToastRestart")
          : t("frostmod.installedToastDesc"),
      });
    } catch (e) {
      // Re-read the real state: a failed install leaves the previous one in place,
      // and the panel shouldn't keep showing whatever it had guessed before.
      await refreshStatus();
      recordActivity({
        title: t("activity.integrationInstallFailed"),
        detail: String(e),
        status: "error",
      });
      toast.error(t("frostmod.installFailed"), { description: String(e) });
    } finally {
      installFlight.current = false;
      setInstalling(false);
    }
  }, [refreshStatus, rememberIntegrationChoice, t]);

  const useAppOnly = useCallback(async () => {
    // App-only is a durable choice, not just a hidden badge. Disable both launch paths
    // used by the backend, then stop an already-running integration for this session.
    try {
      await setAutoRunFrostmod(false);
      if (running) await stop();
      rememberIntegrationChoice("app-only");
      recordActivity({
        title: t("activity.integrationAppOnly"),
        detail: t("activity.integrationAppOnlyDesc"),
        status: "success",
      });
    } catch (e) {
      toast.error(t("frostmod.stopFailed"), { description: String(e) });
    }
  }, [rememberIntegrationChoice, running, stop, t]);

  const enableIntegration = useCallback(async () => {
    rememberIntegrationChoice("enabled");
    try {
      await setAutoRunFrostmod(true);
    } catch (e) {
      // The component can still be installed and started for this session; be honest that
      // Windows may not start it automatically next time.
      toast.warning(t("frostmod.startFailed"), { description: String(e) });
    }
    if (
      !status?.installed ||
      status.needsRepair ||
      !status.supportedForGame ||
      (status.latest !== null && status.version !== status.latest)
    ) {
      await install();
    }
    await start();
    recordActivity({
      title: t("activity.integrationEnabled"),
      detail: t("activity.integrationEnabledDesc"),
      status: "success",
    });
  }, [install, rememberIntegrationChoice, start, status, t]);

  const installRuntime = useCallback(
    async (runtime: VcRuntime) => {
      setInstallingRuntime(true);
      try {
        const outcome = await frostmodInstallRuntime(runtime);
        await refreshStatus();
        if (outcome === "installed") {
          toast.success(t("runtime.installed"), {
            description: t("runtime.installedDesc"),
          });
        } else {
          // Declining the admin prompt isn't a failure — but nothing was fixed either,
          // so hand over the download rather than leaving them at the same dead end.
          toast.info(t("runtime.cancelled"), {
            description: t("runtime.cancelledDesc"),
          });
          void openUrl(RUNTIME_DOWNLOAD_URL[runtime]);
        }
      } catch (e) {
        toast.error(t("runtime.installFailed"), {
          description: String(e),
          action: {
            label: t("runtime.downloadManually"),
            onClick: () => void openUrl(RUNTIME_DOWNLOAD_URL[runtime]),
          },
        });
      } finally {
        setInstallingRuntime(false);
      }
    },
    [refreshStatus, t],
  );

  /**
   * Install everything the PC is short of, and sweep up the stray `msvcr90.dll` older
   * builds of this app left beside the game exe.
   *
   * The one path that doesn't ask detection for permission first. A PC can report every
   * runtime present and still stop the game dead, so this always does the work and reports
   * what it found rather than deciding there was nothing to do.
   *
   * Never throws: the backend returns a report instead, so a UAC prompt declined on one
   * installer doesn't cost the player the other two.
   */
  const repairRuntimes = useCallback(async () => {
    setRepairingRuntimes(true);
    try {
      const report = await frostmodRepairRuntimes();
      await refreshStatus();

      const fixed =
        report.installed.length > 0 || report.strayMsvcr90 === "removed";
      if (report.stillMissing.length > 0) {
        // Partly done at best, and every remaining item has a link. Offer the first —
        // opening three tabs at once would be its own kind of unhelpful.
        const first = report.stillMissing[0];
        toast.warning(t("runtime.repairPartial"), {
          description: t("runtime.repairPartialDesc", {
            what: report.stillMissing.map((r) => t(RUNTIME_NAME_KEY[r])).join(", "),
          }),
          action: {
            label: t("runtime.downloadManually"),
            onClick: () => void openUrl(RUNTIME_DOWNLOAD_URL[first]),
          },
          duration: 12000,
        });
      } else if (fixed) {
        toast.success(t("runtime.repairDone"), {
          description: t("runtime.repairDoneDesc"),
        });
      } else if (!report.gameDirKnown) {
        // Everything was already installed, but with no game folder we never looked at the
        // half of the job that lives in it. Saying "all good" here would be a lie.
        toast.info(t("runtime.repairNoGameFolder"), {
          description: t("runtime.repairNoGameFolderDesc"),
        });
      } else {
        toast.info(t("runtime.repairNothingToDo"), {
          description: t("runtime.repairNothingToDoDesc"),
        });
      }
    } catch (e) {
      toast.error(t("runtime.repairFailed"), {
        description: String(e),
        action: {
          label: t("runtime.downloadManually"),
          onClick: () => void openUrl(RUNTIME_DOWNLOADS_PAGE),
        },
      });
    } finally {
      setRepairingRuntimes(false);
    }
  }, [refreshStatus, t]);

  /**
   * Move the stray `msvcr90.dll` aside, now that the player has asked for it.
   *
   * The one file-destroying thing in here, which is why it is only ever reachable from a
   * bar that has already named the file: the backend refuses to delete a `msvcr90.dll` it
   * can't prove this app planted, and a press is the only other thing that settles it.
   */
  const clearStrayMsvcr90 = useCallback(async () => {
    setClearingStray(true);
    try {
      await frostmodClearStrayMsvcr90();
      await refreshStatus();
      toast.success(t("runtime.strayCleared"), {
        description: t("runtime.strayClearedDesc"),
      });
    } catch (e) {
      // Almost always the game holding the file open, and the backend's message says so.
      toast.error(t("runtime.strayClearFailed"), { description: String(e) });
    } finally {
      setClearingStray(false);
    }
  }, [refreshStatus, t]);

  // A file that crashes the game outranks a runtime that isn't installed, so this is
  // checked before `missingRuntime` wherever one bar has to win. `clear`/`removed` mean
  // there is nothing there — only the two arms that leave a file behind reach the UI.
  const stray = status?.strayMsvcr90;
  const strayMsvcr90 = stray === "foreign" || stray === "locked" ? stray : null;
  const strayWarning = runtimeDismissed ? null : strayMsvcr90;

  // Only ever surface one at a time: two banners stacked over the app is noise, and the
  // game's own runtime (vc90) is the one that produces the error people actually report,
  // so `missingRuntimes` order (vc90 first) decides.
  const missingRuntime = status?.missingRuntimes?.[0] ?? null;
  // The banner respects a dismissal; the Settings panel deliberately doesn't. Dismissing
  // a bar you didn't understand shouldn't also erase the one place that explains it.
  const runtimeWarning =
    integrationChoice === "enabled" && !runtimeDismissed ? missingRuntime : null;

  const dismissRuntimeWarning = useCallback(() => setRuntimeDismissed(true), []);

  // Existing installations predate the consent screen. Treat them as enabled unless the
  // player explicitly chose app-only mode, preserving today's behavior without ever
  // installing an executable onto a machine that did not already have it.
  useEffect(() => {
    if (integrationChoice === null && status?.installed) {
      rememberIntegrationChoice("enabled");
    }
  }, [integrationChoice, rememberIntegrationChoice, status?.installed]);

  // Once enabled, keep the component compatible as before. With no choice or app-only
  // selected, status checks remain read-only and no download or replacement can begin.
  //
  // `missingRuntimes` is deliberately NOT in here. Reinstalling FrostMod cannot put a
  // Visual C++ runtime on the machine, so auto-installing on that flag would download
  // FrostMod again to no effect — and the flag would still be set afterwards. It needs
  // the user's admin consent anyway, so it stays a banner they press.
  const triedRepair = useRef(false);
  const updatedTo = useRef<string | null>(null);
  useEffect(() => {
    if (integrationChoice !== "enabled") return;
    const action = autoInstallAction(status, {
      statusError,
      installing,
      gameRunning,
      triedRepair: triedRepair.current,
      updatedTo: updatedTo.current,
    });
    if (!action) return;
    // Recorded before the install, not after: it must count as attempted even if it
    // throws, or a failure would retry on every poll.
    if (action === "repair") triedRepair.current = true;
    else updatedTo.current = status?.latest ?? null;
    void install();
  }, [status, statusError, installing, gameRunning, install, integrationChoice]);

  const value = useMemo(
    () => ({
      integrationChoice,
      integrationConsentOpen,
      enableIntegration,
      useAppOnly,
      requestIntegrationConsent,
      running,
      attachment,
      status,
      installing,
      checking,
      statusError,
      reload,
      refresh: probe,
      refreshStatus,
      install,
      start,
      stop,
      installingRuntime,
      installRuntime,
      repairingRuntimes,
      repairRuntimes,
      strayMsvcr90,
      strayWarning,
      clearingStray,
      clearStrayMsvcr90,
      dismissRuntimeWarning,
      runtimeWarning,
      missingRuntime,
    }),
    [integrationChoice, integrationConsentOpen, enableIntegration, useAppOnly, requestIntegrationConsent, running, attachment, status, installing, checking, statusError, reload, probe, refreshStatus, install, start, stop, installingRuntime, installRuntime, repairingRuntimes, repairRuntimes, strayMsvcr90, strayWarning, clearingStray, clearStrayMsvcr90, dismissRuntimeWarning, runtimeWarning, missingRuntime],
  );

  return (
    <FrostmodContext.Provider value={value}>
      {children}
    </FrostmodContext.Provider>
  );
}
