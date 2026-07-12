import { useCallback, useEffect, useState } from "react";
import {
  Box,
  Button,
  Chip,
  CircularProgress,
  InputAdornment,
  Skeleton,
  Stack,
  TextField,
  Typography,
} from "@mui/material";
import SearchRoundedIcon from "@mui/icons-material/SearchRounded";
import {
  SEARCH_PAGE_SIZE,
  normalizeModName,
  searchMods,
  type ModType,
} from "../../api/mods";
import type { ModSummary } from "../../types";
import ModCard from "./ModCard";
import "./Browse.scss";

interface BrowseProps {
  modType: ModType;
  installedNames: Set<string>;
  onOpenMod: (slug: string) => void;
}

const Browse = ({ modType, installedNames, onOpenMod }: BrowseProps) => {
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [categoryId, setCategoryId] = useState(modType.categoryId);
  const [mods, setMods] = useState<ModSummary[]>([]);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  // Reset the category filter when the mod type changes.
  useEffect(() => {
    setCategoryId(modType.categoryId);
  }, [modType]);

  // Debounce the search input so we don't hammer the API on every keystroke.
  useEffect(() => {
    const t = setTimeout(() => setDebounced(query.trim()), 350);
    return () => clearTimeout(t);
  }, [query]);

  // (Re)load the first page whenever the query or category changes.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setPage(1);
    searchMods(debounced, categoryId, 1)
      .then((res) => {
        if (cancelled) return;
        setMods(res);
        setHasMore(res.length >= SEARCH_PAGE_SIZE);
      })
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [debounced, categoryId, reloadKey]);

  const loadMore = useCallback(async () => {
    const next = page + 1;
    setLoadingMore(true);
    try {
      const res = await searchMods(debounced, categoryId, next);
      setMods((prev) => [...prev, ...res]);
      setHasMore(res.length >= SEARCH_PAGE_SIZE);
      setPage(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingMore(false);
    }
  }, [debounced, categoryId, page]);

  return (
    <div id={"browse"}>
      <Stack className={"controls"} spacing={2}>
        <TextField
          fullWidth
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={`Search ${modType.label.toLowerCase()}…`}
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
        <Stack direction={"row"} spacing={1} flexWrap={"wrap"} useFlexGap>
          {modType.categories.map((c) => (
            <Chip
              key={c.id}
              label={c.label}
              color={c.id === categoryId ? "primary" : "default"}
              onClick={() => setCategoryId(c.id)}
            />
          ))}
        </Stack>
      </Stack>

      {error ? (
        <Stack className={"state"} spacing={2} alignItems={"center"}>
          <Typography color={"error"}>Couldn&apos;t load mods: {error}</Typography>
          <Button variant={"outlined"} onClick={() => setReloadKey((k) => k + 1)}>
            Retry
          </Button>
        </Stack>
      ) : loading ? (
        <Box className={"grid"}>
          {Array.from({ length: 8 }).map((_, i) => (
            <Skeleton
              key={i}
              variant={"rounded"}
              height={200}
              animation={"wave"}
            />
          ))}
        </Box>
      ) : mods.length === 0 ? (
        <Typography className={"state"} color={"text.secondary"}>
          No {modType.label.toLowerCase()} found.
        </Typography>
      ) : (
        <>
          <Box className={"grid"}>
            {mods.map((m) => (
              <ModCard
                key={m.id}
                mod={m}
                installed={installedNames.has(normalizeModName(m.title))}
                onClick={() => onOpenMod(m.slug)}
              />
            ))}
          </Box>
          {hasMore && (
            <Box className={"load-more"}>
              <Button
                variant={"outlined"}
                onClick={loadMore}
                disabled={loadingMore}
                startIcon={
                  loadingMore ? <CircularProgress size={16} /> : undefined
                }
              >
                {loadingMore ? "Loading…" : "Load more"}
              </Button>
            </Box>
          )}
        </>
      )}
    </div>
  );
};

export default Browse;
