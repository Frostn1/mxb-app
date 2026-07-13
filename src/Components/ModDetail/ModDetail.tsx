import { useEffect, useMemo, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  LinearProgress,
  Stack,
  Typography,
} from "@mui/material";
import ArrowBackIosNewRoundedIcon from "@mui/icons-material/ArrowBackIosNewRounded";
import DownloadRoundedIcon from "@mui/icons-material/DownloadRounded";
import OpenInNewRoundedIcon from "@mui/icons-material/OpenInNewRounded";
import CheckCircleRoundedIcon from "@mui/icons-material/CheckCircleRounded";
import UploadFileRoundedIcon from "@mui/icons-material/UploadFileRounded";
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
  isManualHost,
  normalizeModName,
  onInstallProgress,
  type ModType,
} from "../../api/mods";
import type { DownloadOption, InstallProgress, ModDetail as Detail } from "../../types";
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
  const [manualHint, setManualHint] = useState(false);

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

  // Auto-installable hosts first, so the primary button is a one-click install.
  const downloads = useMemo(
    () =>
      detail
        ? [...detail.downloads].sort(
            (a, b) => Number(isManualHost(a.host)) - Number(isManualHost(b.host)),
          )
        : [],
    [detail],
  );

  const handleInstall = async (opt: DownloadOption) => {
    setInstalling(true);
    setProgress({ slug, stage: "resolving" });
    const unlisten = await onInstallProgress((p) => {
      if (p.slug === slug) setProgress(p);
    });
    try {
      await addToLibrary(slug, opt.url, opt.host, modType.installSubpath);
      setProgress({ slug, stage: "done" });
      onInstalled();
    } catch (e) {
      setProgress({ slug, stage: "error", message: String(e) });
    } finally {
      unlisten();
      setInstalling(false);
    }
  };

  // MediaFire/Mega block in-app downloads — open in the browser to grab the
  // file, then the user imports it below.
  const handleDownload = (opt: DownloadOption) => {
    if (isManualHost(opt.host)) {
      setManualHint(true);
      open(opt.url);
      return;
    }
    handleInstall(opt);
  };

  const handleImport = async () => {
    const picked = await pickFile({
      multiple: false,
      filters: [{ name: "Mod files", extensions: ["pkz", "zip", "rar", "7z"] }],
    });
    if (typeof picked !== "string") return;
    setInstalling(true);
    setProgress({ slug, stage: "placing" });
    try {
      await importFile(picked, modType.installSubpath);
      setProgress({ slug, stage: "done" });
      onInstalled();
    } catch (e) {
      setProgress({ slug, stage: "error", message: String(e) });
    } finally {
      setInstalling(false);
    }
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

      <Stack
        direction={"row"}
        spacing={1.5}
        alignItems={"center"}
        sx={{ mt: 1 }}
      >
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
        {detail.downloads.length === 0 && (
          <Alert severity={"info"}>
            No download link was found on this page — open it on mxb-mods.com.
          </Alert>
        )}
        <Stack direction={"row"} spacing={1.5} flexWrap={"wrap"} useFlexGap>
          {downloads.map((opt, i) => {
            const manual = isManualHost(opt.host);
            return (
              <Button
                key={`${opt.url}-${i}`}
                variant={i === 0 ? "contained" : "outlined"}
                startIcon={
                  manual ? <OpenInNewRoundedIcon /> : <DownloadRoundedIcon />
                }
                disabled={installing}
                onClick={() => handleDownload(opt)}
              >
                {manual
                  ? `Download · ${opt.host}`
                  : i === 0
                    ? "Add to Library"
                    : `Mirror · ${opt.host}`}
              </Button>
            );
          })}
          <Button
            variant={"text"}
            startIcon={<UploadFileRoundedIcon />}
            disabled={installing}
            onClick={handleImport}
          >
            Import a file
          </Button>
        </Stack>

        {manualHint && (
          <Alert severity={"info"} sx={{ mt: 2 }}>
            This host blocks in-app downloads, so it opened in your browser.
            Once it&apos;s downloaded, click <b>Import a file</b> and pick it —
            it&apos;ll be added to your {modType.label.toLowerCase()}.
          </Alert>
        )}

        {progress && (
          <Box className={"progress"} sx={{ mt: 2 }}>
            {progress.stage === "done" ? (
              <Alert icon={<CheckCircleRoundedIcon />} severity={"success"}>
                {STAGE_LABEL.done}
              </Alert>
            ) : progress.stage === "error" ? (
              <Alert severity={"error"}>
                {progress.message ?? STAGE_LABEL.error}
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
