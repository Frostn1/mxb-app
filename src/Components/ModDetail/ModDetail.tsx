import { useEffect, useMemo, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  LinearProgress,
  Link,
  Stack,
  Typography,
} from "@mui/material";
import ArrowBackIosNewRoundedIcon from "@mui/icons-material/ArrowBackIosNewRounded";
import DownloadRoundedIcon from "@mui/icons-material/DownloadRounded";
import OpenInNewRoundedIcon from "@mui/icons-material/OpenInNewRounded";
import CheckCircleRoundedIcon from "@mui/icons-material/CheckCircleRounded";
import ContentCopyRoundedIcon from "@mui/icons-material/ContentCopyRounded";
import { Swiper, SwiperSlide } from "swiper/react";
import { Navigation, Pagination } from "swiper/modules";
import { open } from "@tauri-apps/plugin-shell";
import { open as pickFile } from "@tauri-apps/plugin-dialog";
import "swiper/css";
import "swiper/css/navigation";
import "swiper/css/pagination";
import {
  addToLibrary,
  getModDetail,
  importFile,
  isBlockedDownload,
  normalizeModName,
  onFrostmodReload,
  onInstallProgress,
  type ModType,
} from "../../api/mods";
import type {
  DownloadOption,
  InstallProgress,
  ModDetail as Detail,
  ReloadOutcome,
} from "../../types";
import "./ModDetail.scss";

interface ModDetailProps {
  slug: string;
  modType: ModType;
  installedNames: Set<string>;
  onBack: () => void;
  onInstalled: () => void;
}

const STAGE_LABEL: Record<InstallProgress["stage"], string> = {
  resolving: "Resolving download…",
  downloading: "Downloading…",
  extracting: "Extracting…",
  placing: "Adding to library…",
  done: "Added to your library",
  error: "Something went wrong",
};

const ModDetail = ({
  slug,
  modType,
  installedNames,
  onBack,
  onInstalled,
}: ModDetailProps) => {
  const [detail, setDetail] = useState<Detail | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [frostmod, setFrostmod] = useState<ReloadOutcome | null>(null);
  const [manualHint, setManualHint] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setLoadError(null);
    setProgress(null);
    setManualHint(false);
    getModDetail(slug)
      .then((d) => !cancelled && setDetail(d))
      .catch((e) => !cancelled && setLoadError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [slug]);

  // The "official" download to feature: prefer a one-click (auto) host among the
  // author's default files; servers are never primary.
  const { primary, others } = useMemo(() => {
    const all = detail?.downloads ?? [];
    if (all.length === 0) return { primary: null, others: [] as DownloadOption[] };
    const playable = all.filter((d) => !d.isServer);
    const pool = playable.length ? playable : all;
    const auto = pool.filter((d) => !isBlockedDownload(d));
    const pick =
      auto.find((d) => d.isDefault) ??
      auto[0] ??
      pool.find((d) => d.isDefault) ??
      pool[0] ??
      null;
    return { primary: pick, others: all.filter((d) => d !== pick) };
  }, [detail]);

  const runInstall = async (fn: () => Promise<void>) => {
    setInstalling(true);
    setProgress({ slug, stage: "resolving" });
    setFrostmod(null);
    const unlisten = await onInstallProgress((p) => {
      if (p.slug === slug) setProgress(p);
    });
    // The backend signals FrostMod after placing the files; capture whether the
    // new mod went live in-game so we can tell the user.
    const unlistenFrostmod = await onFrostmodReload((p) => {
      if (p.slug === slug) setFrostmod(p.outcome);
    });
    try {
      await fn();
      setProgress({ slug, stage: "done" });
      onInstalled();
    } catch (e) {
      setProgress({ slug, stage: "error", message: String(e) });
    } finally {
      unlisten();
      unlistenFrostmod();
      setInstalling(false);
    }
  };

  const install = (opt: DownloadOption) =>
    runInstall(() =>
      addToLibrary(slug, opt.url, opt.host, modType.installSubpath),
    );

  // Blocked hosts (MediaFire/Mega) can't be fetched in-app — open in the browser,
  // then reveal the import step.
  const openInBrowser = (opt: DownloadOption) => {
    setManualHint(true);
    open(opt.url);
  };

  const chooseAndImport = async () => {
    const picked = await pickFile({
      multiple: false,
      filters: [{ name: "Mod files", extensions: ["pkz", "zip", "rar", "7z"] }],
    });
    if (typeof picked !== "string") return;
    await runInstall(() => importFile(picked, modType.installSubpath));
  };

  const clickDownload = (opt: DownloadOption) =>
    isBlockedDownload(opt) ? openInBrowser(opt) : install(opt);

  const copyError = () => {
    if (!progress?.message) return;
    navigator.clipboard.writeText(progress.message);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  if (loadError) {
    return (
      <div id={"mod-detail"}>
        <BackButton onBack={onBack} />
        <Alert severity={"error"} sx={{ mt: 2 }}>
          Couldn&apos;t load this mod: {loadError}
        </Alert>
      </div>
    );
  }

  if (!detail) {
    return (
      <div id={"mod-detail"}>
        <BackButton onBack={onBack} />
        <Box className={"loading"}>
          <CircularProgress />
        </Box>
      </div>
    );
  }

  const pct =
    progress?.total && progress.received
      ? Math.round((progress.received / progress.total) * 100)
      : undefined;

  return (
    <div id={"mod-detail"}>
      <Stack
        direction={"row"}
        justifyContent={"space-between"}
        alignItems={"center"}
      >
        <BackButton onBack={onBack} />
        <Button
          size={"small"}
          endIcon={<OpenInNewRoundedIcon />}
          onClick={() => open(detail.link)}
        >
          View on mxb-mods.com
        </Button>
      </Stack>

      <Stack direction={"row"} spacing={1.5} alignItems={"center"} sx={{ mt: 1 }}>
        <Typography variant={"h5"}>{detail.title}</Typography>
        {detail.version && <Chip size={"small"} label={detail.version} />}
        {installedNames.has(normalizeModName(detail.title)) && (
          <Chip
            size={"small"}
            color={"success"}
            icon={<CheckCircleRoundedIcon />}
            label={"In library"}
          />
        )}
      </Stack>
      <Typography variant={"caption"} color={"text.secondary"}>
        {new Date(detail.date).toLocaleDateString()}
      </Typography>

      {detail.images.length > 0 && (
        <Swiper
          className={"gallery"}
          modules={[Navigation, Pagination]}
          navigation
          pagination={{ clickable: true }}
          spaceBetween={12}
        >
          {detail.images.map((src) => (
            <SwiperSlide key={src}>
              <img src={src} alt={detail.title} />
            </SwiperSlide>
          ))}
        </Swiper>
      )}

      <div className={"downloads"}>
        <Typography variant={"subtitle2"} gutterBottom>
          Add to library
        </Typography>

        {!primary && (
          <Alert severity={"info"}>
            No download link was found on this page — open it on mxb-mods.com.
          </Alert>
        )}

        {primary &&
          (isBlockedDownload(primary) ? (
            <Stack spacing={0.75} alignItems={"flex-start"}>
              <Button
                variant={"contained"}
                startIcon={<OpenInNewRoundedIcon />}
                disabled={installing}
                onClick={() => openInBrowser(primary)}
              >
                Download from {primary.host}
              </Button>
              <Typography variant={"caption"} color={"text.secondary"}>
                {primary.host} blocks in-app downloads, so it opens in your
                browser.
              </Typography>
            </Stack>
          ) : (
            <Button
              variant={"contained"}
              startIcon={<DownloadRoundedIcon />}
              disabled={installing}
              onClick={() => install(primary)}
            >
              Add to Library
            </Button>
          ))}

        {manualHint && (
          <Alert
            severity={"info"}
            sx={{ mt: 2 }}
            action={
              <Button color={"inherit"} size={"small"} onClick={chooseAndImport}>
                Select file
              </Button>
            }
          >
            Downloaded it? Click <b>Select file</b> and pick it — it&apos;ll be
            added to your {modType.label.toLowerCase()}.
          </Alert>
        )}

        {others.length > 0 && (
          <Box className={"other-downloads"}>
            <Typography variant={"caption"} color={"text.secondary"}>
              Other downloads
            </Typography>
            <Stack spacing={0.25} sx={{ mt: 0.5 }}>
              {others.map((opt, i) => (
                <Typography key={`${opt.url}-${i}`} variant={"body2"}>
                  <Link
                    component={"button"}
                    type={"button"}
                    disabled={installing}
                    onClick={() => clickDownload(opt)}
                  >
                    {opt.host}
                  </Link>
                  <Typography
                    component={"span"}
                    variant={"caption"}
                    color={"text.secondary"}
                  >
                    {opt.isServer
                      ? " — server version, not needed for normal play"
                      : isBlockedDownload(opt)
                        ? " — opens in your browser"
                        : ""}
                  </Typography>
                </Typography>
              ))}
            </Stack>
          </Box>
        )}

        {progress && (
          <Box className={"progress"} sx={{ mt: 2 }}>
            {progress.stage === "done" ? (
              <Stack spacing={1}>
                <Alert icon={<CheckCircleRoundedIcon />} severity={"success"}>
                  {STAGE_LABEL.done}
                </Alert>
                {frostmod === "signaled" && (
                  <Alert severity={"success"} variant={"outlined"}>
                    FrostMod reloaded the game — this mod is live now.
                  </Alert>
                )}
                {frostmod === "not_running" && (
                  <Alert severity={"info"} variant={"outlined"}>
                    Start FrostMod to load new mods without restarting, or press{" "}
                    R in its console / F8 in‑game. Otherwise it'll be there next
                    time you launch MX Bikes.
                  </Alert>
                )}
              </Stack>
            ) : progress.stage === "error" ? (
              <Alert
                severity={"error"}
                action={
                  <Button
                    color={"inherit"}
                    size={"small"}
                    startIcon={<ContentCopyRoundedIcon />}
                    onClick={copyError}
                  >
                    {copied ? "Copied" : "Copy"}
                  </Button>
                }
              >
                <span className={"selectable"}>
                  {progress.message ?? STAGE_LABEL.error}
                </span>
              </Alert>
            ) : (
              <>
                <Typography variant={"caption"} color={"text.secondary"}>
                  {STAGE_LABEL[progress.stage]}
                  {pct !== undefined ? ` ${pct}%` : ""}
                </Typography>
                <LinearProgress
                  variant={pct !== undefined ? "determinate" : "indeterminate"}
                  value={pct}
                />
              </>
            )}
          </Box>
        )}
      </div>

      <div
        className={"description"}
        // Content is authored HTML from mxb-mods.com's REST API.
        dangerouslySetInnerHTML={{ __html: detail.descriptionHtml }}
      />
    </div>
  );
};

const BackButton = ({ onBack }: { onBack: () => void }) => (
  <Button
    size={"small"}
    startIcon={<ArrowBackIosNewRoundedIcon />}
    onClick={onBack}
  >
    Back
  </Button>
);

export default ModDetail;
