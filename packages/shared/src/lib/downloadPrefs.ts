/**
 * Which download to take when a mod offers several.
 *
 * A mod on mxb-mods often ships the same file on two or three hosts, and sometimes a
 * dedicated-server build beside the playable one. The app already ranks them — server builds
 * last, hosts we can't fetch unattended below the rest — but that ranking is fixed, and two
 * people want opposite things from it:
 *
 * - someone running a dedicated server wants the server build *every time*, and today has to
 *   open each mod and pick it by hand, because quick install refuses a server-only mod
 *   outright rather than guess;
 * - a Google Drive link that has been shared widely returns "too many downloads", so the
 *   MediaFire copy beside it is the one that actually works — and today that is a manual
 *   pick on every mod too.
 *
 * Both are one stored preference, applied wherever a mirror gets chosen.
 */

/** What the user wants taken when there is a choice. */
export interface DownloadPrefs {
  /**
   * Prefer the dedicated-server build where a mod offers one.
   *
   * Off by default: a server file installs cleanly and then the game shows nothing, so it is
   * the wrong guess for almost everyone. On, it also lets quick install take a server-only
   * mod rather than refusing it.
   */
  preferServer: boolean;
  /** {@link KNOWN_HOSTS} id to prefer, or `""` for no preference. */
  preferredHost: string;
}

export const DEFAULT_DOWNLOAD_PREFS: DownloadPrefs = {
  preferServer: false,
  preferredHost: "",
};

/**
 * The hosts worth naming, and the fragments that identify one.
 *
 * `DownloadOption.host` is a label scraped off the page, not a domain — "Media Fire" and
 * "drive.google.com" are both real values — so matching normalises away everything but
 * letters and digits and then looks for a fragment.
 */
export const KNOWN_HOSTS: { id: string; label: string; match: string[] }[] = [
  { id: "mediafire", label: "MediaFire", match: ["mediafire"] },
  { id: "gdrive", label: "Google Drive", match: ["googledrive", "drivegoogle", "docsgoogle"] },
  { id: "mega", label: "MEGA", match: ["meganz", "megaio"] },
  { id: "dropbox", label: "Dropbox", match: ["dropbox"] },
  { id: "onedrive", label: "OneDrive", match: ["onedrive", "1drv"] },
  { id: "discord", label: "Discord", match: ["discord"] },
  { id: "github", label: "GitHub", match: ["github"] },
];

const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");

/** Whether a download sits on the named {@link KNOWN_HOSTS} entry. */
export function isHost(opt: { url: string; host: string }, id: string): boolean {
  const known = KNOWN_HOSTS.find((h) => h.id === id);
  if (!known) return false;
  const hay = norm(`${opt.host} ${opt.url}`);
  return known.match.some((m) => hay.includes(m));
}

const KEY = "frost-download-prefs";

export function readDownloadPrefs(): DownloadPrefs {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return DEFAULT_DOWNLOAD_PREFS;
    const v = JSON.parse(raw) as Partial<DownloadPrefs>;
    return {
      preferServer: v.preferServer === true,
      preferredHost:
        typeof v.preferredHost === "string" &&
        KNOWN_HOSTS.some((h) => h.id === v.preferredHost)
          ? v.preferredHost
          : "",
    };
  } catch {
    // A browser with storage blocked, or someone's hand-edited JSON. The defaults are the
    // behaviour the app had before this existed, so falling back to them is always safe.
    return DEFAULT_DOWNLOAD_PREFS;
  }
}

export function writeDownloadPrefs(prefs: DownloadPrefs) {
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    /* storage blocked — the pick just won't outlive the session */
  }
}
