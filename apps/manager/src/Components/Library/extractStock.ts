import { save as pickSavePath } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { extractStockTrack } from "@frost/shared/api/mods";
import type { LibraryEntry } from "@frost/shared/types";
import { formatBytes } from "@frost/shared/lib/mods";
import type { TFunc } from "@frost/shared/i18n/core";
import type { TKey } from "@/i18n";

/**
 * Lift one stock track out of the install's shared `tracks.pkz` into a `.pkz` of its own.
 *
 * Where it goes is the player's choice and the dialog defaults to nothing in particular,
 * deliberately: the mods tree is the one place this must not land by accident, because a mod
 * track sharing a stock track's id gives the game two sources for one name.
 *
 * Shared by the card menu and the detail page so the two can't drift on what "Extract" means.
 * Resolves to the path written, or `null` when the player cancelled.
 */
export async function extractStock(
  entry: LibraryEntry,
  t: TFunc<TKey>,
): Promise<string | null> {
  const to = await pickSavePath({
    defaultPath: `${entry.name}.pkz`,
    filters: [{ name: "MX Bikes track", extensions: ["pkz"] }],
  });
  if (!to) return null;

  const pending = toast.loading(t("library.extractingStock", { name: entry.name }));
  try {
    const bytes = await extractStockTrack(entry.name, to);
    toast.success(t("library.extractedStock", { size: formatBytes(bytes) }), { id: pending });
    return to;
  } catch (e) {
    toast.error(e instanceof Error ? e.message : String(e), { id: pending });
    return null;
  }
}
