import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Box,
  Button,
  Card,
  CardContent,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  IconButton,
  InputAdornment,
  Menu,
  MenuItem,
  Stack,
  TextField,
  Tooltip,
  Typography,
} from "@mui/material";
import RefreshRoundedIcon from "@mui/icons-material/RefreshRounded";
import SearchRoundedIcon from "@mui/icons-material/SearchRounded";
import TwoWheelerRoundedIcon from "@mui/icons-material/TwoWheelerRounded";
import TerrainRoundedIcon from "@mui/icons-material/TerrainRounded";
import FolderRoundedIcon from "@mui/icons-material/FolderRounded";
import DriveFileMoveRoundedIcon from "@mui/icons-material/DriveFileMoveRounded";
import CreateNewFolderRoundedIcon from "@mui/icons-material/CreateNewFolderRounded";
import { getInstalledMods, moveMod, type ModType } from "../../api/mods";
import type { InstalledMod } from "../../types";
import "./Library.scss";

interface LibraryProps {
  modType: ModType;
  /** Bumped by the Dashboard after an install to force a re-scan. */
  refreshKey: number;
}

const displayName = (name: string) => name.replace(/\.(pkz|zip|rar|7z)$/i, "");
const folderLabel = (folder: string) => folder || "(root)";

const Library = ({ modType, refreshKey }: LibraryProps) => {
  const [items, setItems] = useState<InstalledMod[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState<{ anchor: HTMLElement; item: InstalledMod } | null>(null);
  const [newFolder, setNewFolder] = useState<{ item: InstalledMod; name: string } | null>(null);

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

  const allFolders = useMemo(
    () => [...new Set(items.map((i) => i.folder))].sort((a, b) => a.localeCompare(b)),
    [items],
  );

  const groups = useMemo(() => {
    const q = search.trim().toLowerCase();
    const filtered = q
      ? items.filter(
          (i) =>
            i.name.toLowerCase().includes(q) || i.folder.toLowerCase().includes(q),
        )
      : items;
    const map = new Map<string, InstalledMod[]>();
    for (const it of filtered) {
      const list = map.get(it.folder) ?? [];
      list.push(it);
      map.set(it.folder, list);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [items, search]);

  const doMove = async (item: InstalledMod, toFolder: string) => {
    setBusy(true);
    setMenu(null);
    setNewFolder(null);
    try {
      await moveMod(item.path, toFolder, modType.installSubpath);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const TypeIcon = modType.id === "bikes" ? TwoWheelerRoundedIcon : TerrainRoundedIcon;

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
          disabled={loading || busy}
        >
          Refresh
        </Button>
      </Stack>

      <TextField
        fullWidth
        size={"small"}
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        placeholder={`Search installed ${modType.label.toLowerCase()}…`}
        sx={{ mb: 2 }}
        slotProps={{
          input: {
            startAdornment: (
              <InputAdornment position={"start"}>
                <SearchRoundedIcon />
              </InputAdornment>
            ),
          },
        }}
      />

      {error && (
        <Typography color={"error"} className={"selectable"}>
          {error}
        </Typography>
      )}

      {loading ? (
        <Box className={"state"}>
          <CircularProgress />
        </Box>
      ) : groups.length === 0 && !error ? (
        <Typography className={"state"} color={"text.secondary"}>
          {items.length === 0
            ? `No ${modType.label.toLowerCase()} installed yet — head to Browse and add one.`
            : "No matches."}
        </Typography>
      ) : (
        groups.map(([folder, mods]) => (
          <section key={folder} className={"folder-group"}>
            <div className={"folder-title"}>
              <FolderRoundedIcon fontSize={"small"} />
              <span>{folderLabel(folder)}</span>
              <Typography component={"span"} variant={"caption"} color={"text.secondary"}>
                {mods.length}
              </Typography>
            </div>
            <Box className={"grid"}>
              {mods.map((item) => (
                <Card key={item.path} className={"folder-card"}>
                  <CardContent>
                    <Stack direction={"row"} alignItems={"center"} spacing={1}>
                      <TypeIcon color={"primary"} />
                      <Typography
                        variant={"subtitle1"}
                        noWrap
                        title={item.name}
                        sx={{ flex: 1 }}
                      >
                        {displayName(item.name)}
                      </Typography>
                      <Tooltip title={"Move to folder"}>
                        <span>
                          <IconButton
                            size={"small"}
                            disabled={busy}
                            onClick={(e) => setMenu({ anchor: e.currentTarget, item })}
                          >
                            <DriveFileMoveRoundedIcon fontSize={"small"} />
                          </IconButton>
                        </span>
                      </Tooltip>
                    </Stack>
                  </CardContent>
                </Card>
              ))}
            </Box>
          </section>
        ))
      )}

      <Menu
        anchorEl={menu?.anchor ?? null}
        open={Boolean(menu)}
        onClose={() => setMenu(null)}
      >
        <Typography variant={"caption"} color={"text.secondary"} sx={{ px: 2, py: 0.5 }}>
          Move to…
        </Typography>
        {menu?.item.folder !== "" && (
          <MenuItem onClick={() => menu && doMove(menu.item, "")}>(root)</MenuItem>
        )}
        {menu &&
          allFolders
            .filter((f) => f !== "" && f !== menu.item.folder)
            .map((f) => (
              <MenuItem key={f} onClick={() => doMove(menu.item, f)}>
                {f}
              </MenuItem>
            ))}
        <MenuItem
          onClick={() => {
            if (menu) setNewFolder({ item: menu.item, name: "" });
            setMenu(null);
          }}
        >
          <CreateNewFolderRoundedIcon fontSize={"small"} sx={{ mr: 1 }} />
          New folder…
        </MenuItem>
      </Menu>

      <Dialog open={Boolean(newFolder)} onClose={() => setNewFolder(null)}>
        <DialogTitle>New folder</DialogTitle>
        <DialogContent>
          <TextField
            autoFocus
            fullWidth
            variant={"standard"}
            label={"Folder name"}
            value={newFolder?.name ?? ""}
            onChange={(e) =>
              setNewFolder((s) => (s ? { ...s, name: e.target.value } : s))
            }
            onKeyDown={(e) => {
              if (e.key === "Enter" && newFolder?.name.trim()) {
                doMove(newFolder.item, newFolder.name.trim());
              }
            }}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setNewFolder(null)}>Cancel</Button>
          <Button
            disabled={!newFolder?.name.trim()}
            onClick={() =>
              newFolder && doMove(newFolder.item, newFolder.name.trim())
            }
          >
            Create &amp; move
          </Button>
        </DialogActions>
      </Dialog>
    </div>
  );
};

export default Library;
