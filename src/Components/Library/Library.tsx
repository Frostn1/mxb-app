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
import FolderRoundedIcon from "@mui/icons-material/FolderRounded";
import { getInstalledMods, type ModType } from "../../api/mods";
import type { InstalledMod } from "../../types";
import "./Library.scss";

interface LibraryProps {
  modType: ModType;
  /** Bumped by the Dashboard after an install to force a re-scan. */
  refreshKey: number;
}

/** Drop the archive-style extension for display. */
const displayName = (name: string) => name.replace(/\.(pkz|zip|rar|7z)$/i, "");

const Library = ({ modType, refreshKey }: LibraryProps) => {
  const [items, setItems] = useState<InstalledMod[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setItems(await getInstalledMods(modType.installSubpath));
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
          {items.length > 0 && (
            <Typography component={"span"} color={"text.secondary"}>
              {" "}
              ({items.length})
            </Typography>
          )}
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
      ) : items.length === 0 && !error ? (
        <Typography className={"state"} color={"text.secondary"}>
          No {modType.label.toLowerCase()} installed yet — head to Browse and add
          one.
        </Typography>
      ) : (
        <Box className={"grid"}>
          {items.map((item) => (
            <Card key={item.path} className={"folder-card"}>
              <CardContent>
                <Stack direction={"row"} spacing={1} alignItems={"center"}>
                  <TwoWheelerRoundedIcon color={"primary"} />
                  <Typography
                    variant={"subtitle1"}
                    noWrap
                    title={item.name}
                  >
                    {displayName(item.name)}
                  </Typography>
                </Stack>
                {item.folder && (
                  <Chip
                    className={"folder-chip"}
                    size={"small"}
                    variant={"outlined"}
                    icon={<FolderRoundedIcon />}
                    label={item.folder}
                    title={item.folder}
                    sx={{ mt: 1, maxWidth: "100%" }}
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
