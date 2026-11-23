import { invoke } from "@tauri-apps/api/core";
import type { GameFolderCheck } from "@frost/shared/api/mods";
import type { Config } from "@frost/shared/types";

export function studioInspectModsFolder(path: string): Promise<GameFolderCheck> {
  return invoke<GameFolderCheck>("studio_inspect_mods_folder", { path });
}

export function studioDetectModsFolder(): Promise<string | null> {
  return invoke<string | null>("studio_detect_mods_folder");
}

export function studioSetModsPath(path: string): Promise<Config> {
  return invoke<Config>("studio_set_mods_path", { path });
}
