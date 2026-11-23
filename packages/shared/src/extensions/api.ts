import { invoke } from "@tauri-apps/api/core";
import type { InspectedPackage, InstalledExtension } from "./manifest";

/** The backend half (`crates/core/src/extensions.rs`). Every call works offline. */

export interface ExtListResponse {
  developerMode: boolean;
  extensions: InstalledExtension[];
}

export interface UpdateOutcome {
  id: string;
  status: "updated" | "needs-approval" | "current" | "failed";
  version: string | null;
  message: string | null;
}

export const extList = () => invoke<ExtListResponse>("ext_list");
/** A `.mxbext` file, or (developer mode) a folder. Nothing is installed yet. */
export const extInspectFile = (path: string) => invoke<InspectedPackage>("ext_inspect_file", { path });
export const extInspectUrl = (url: string) => invoke<InspectedPackage>("ext_inspect_url", { url });
export const extInstall = (token: string, granted: string[]) =>
  invoke<string>("ext_install", { token, granted });
export const extSetEnabled = (id: string, enabled: boolean) =>
  invoke<void>("ext_set_enabled", { id, enabled });
export const extSetAutoUpdate = (id: string, on: boolean) => invoke<void>("ext_set_auto_update", { id, on });
export const extSetPermissions = (id: string, granted: string[]) =>
  invoke<void>("ext_set_permissions", { id, granted });
export const extUninstall = (id: string) => invoke<void>("ext_uninstall", { id });
export const extSetDeveloperMode = (on: boolean) => invoke<void>("ext_set_developer_mode", { on });
export const extReadText = (id: string, path: string) => invoke<string>("ext_read_text", { id, path });
export const extReadAsset = (id: string, path: string) =>
  invoke<{ mime: string; base64: string }>("ext_read_asset", { id, path }).then(
    (a) => `data:${a.mime};base64,${a.base64}`,
  );
/** MXB Coach's sessions, shaped for extensions. The bridge gates it behind `coach.read`. */
export const extCoachSessions = () => invoke<import("./data").ExtCoachSession[]>("ext_coach_sessions");
// --- the marketplace --------------------------------------------------------------------

export interface CatalogCard {
  id: string;
  name: string;
  author: string;
  summary: string;
  version: string;
  verified: boolean;
  suggested: boolean;
  categories: string[];
  tags: string[];
  installs: number;
  rating: number | null;
  ratings: number;
  permissions: string[];
  thumbnail: string | null;
  updatedAt: number;
}

export interface CatalogDetail extends CatalogCard {
  description: string;
  homepage: string | null;
  network: string[];
  size: number;
  key: string;
  createdAt: number;
  screenshots: string[];
}

export type CatalogTier = "suggested" | "verified" | "";

export const catalogSearch = (q: string, category: string, sort: string, offset = 0, tier: CatalogTier = "") =>
  invoke<{ extensions: CatalogCard[]; more: boolean }>("ext_catalog_search", { q, category, sort, offset, tier });
export const catalogGet = (id: string) => invoke<CatalogDetail>("ext_catalog_get", { id });
export const catalogLatest = () => invoke<{ extensions: CatalogCard[] }>("ext_catalog_latest");
export const catalogInspect = (id: string) => invoke<InspectedPackage>("ext_catalog_inspect", { id });
export const catalogInstalled = (id: string) => invoke<void>("ext_catalog_installed", { id });
export const catalogRate = (id: string, stars: number) =>
  invoke<{ rating: number | null; ratings: number; mine: number }>("ext_catalog_rate", { id, stars });
export const catalogMyRating = (id: string) => invoke<number | null>("ext_catalog_my_rating", { id });

// --- AI agents (MCP) ------------------------------------------------------------------------

export interface McpStatus {
  enabled: boolean;
  listening: boolean;
  /** This app's executable, for the client config. */
  command: string;
}
export const mcpStatus = () => invoke<McpStatus>("ext_mcp_status");
export const setMcpEnabled = (on: boolean) => invoke<void>("ext_set_mcp_enabled", { on });
export const mcpReply = (id: number, ok: boolean, result: unknown) => invoke<void>("ext_mcp_reply", { id, ok, result });

export const extCheckUpdates = (only?: string) =>
  invoke<UpdateOutcome[]>("ext_check_updates", { only: only ?? null });
