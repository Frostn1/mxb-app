import { useCallback, useEffect, useState } from "react";
import {
  Box,
  Button,
  Card,
  CardContent,
  Chip,
  CircularProgress,
  Stack,
  Typography,
} from "@mui/material";
import RefreshRoundedIcon from "@mui/icons-material/RefreshRounded";
import TwoWheelerRoundedIcon from "@mui/icons-material/TwoWheelerRounded";
import { getInstalledMods, type ModType } from "../../api/mods";
import type { InstalledModFolder } from "../../types";
import "./Library.scss";

interface LibraryProps {
  modType: ModType;
  /** Bumped by the Dashboard after an install to force a re-scan. */
  refreshKey: number;
}

const Library = ({ modType, refreshKey }: LibraryProps) => {
  const [folders, setFolders] = useState<InstalledModFolder[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setFolders(await getInstalledMods(modType.installSubpath));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [modType]);

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  return (
    <div id={"library"}>
      <Stack
        direction={"row"}
        justifyContent={"space-between"}
        alignItems={"center"}
        sx={{ mb: 2 }}
      >
        <Typography variant={"h6"}>
          Installed {modType.label.toLowerCase()}
        </Typography>
        <Button
          size={"small"}
          startIcon={<RefreshRoundedIcon />}
          onClick={load}
          disabled={loading}
        >
          Refresh
        </Button>
      </Stack>

      {error && (
        <Typography color={"error"}>Couldn&apos;t read library: {error}</Typography>
      )}

      {loading ? (
        <Box className={"state"}>
          <CircularProgress />
        </Box>
      ) : folders.length === 0 && !error ? (
        <Typography className={"state"} color={"text.secondary"}>
          No {modType.label.toLowerCase()} installed yet — head to Browse and add
          one.
        </Typography>
      ) : (
        <Box className={"grid"}>
          {folders.map((folder) => (
            <Card key={folder.path} className={"folder-card"}>
              <CardContent>
                <Stack direction={"row"} spacing={1} alignItems={"center"}>
                  <TwoWheelerRoundedIcon color={"primary"} />
                  <Typography variant={"subtitle1"} noWrap title={folder.name}>
                    {folder.name}
                  </Typography>
                </Stack>
                {folder.mods.length > 0 && (
                  <Chip
                    size={"small"}
                    sx={{ mt: 1 }}
                    label={`${folder.mods.length} file${folder.mods.length === 1 ? "" : "s"}`}
                  />
                )}
              </CardContent>
            </Card>
          ))}
        </Box>
      )}
    </div>
  );
};

export default Library;
