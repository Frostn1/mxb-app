import { useCallback, useEffect, useRef, useState } from "react";
import { ExternalLink, Loader2, RefreshCw, Trophy, ArrowUp, ArrowDown, Pencil } from "lucide-react";
import { toast } from "sonner";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import CachedImg from "@frost/shared/Components/ui/cached-img";
import { ContextBarRight } from "../Shell/ContextBar";
import {
  rankedIdentity,
  rankedProfile,
  setRankedGuid,
  type RankedProfile,
  type RankedIdentity,
} from "../../api/ranked";
import { useConfig } from "@frost/shared/Context/Config";
import { useT } from "@/i18n";
import GuidDialog from "./GuidDialog";

/**
 * The player's standing on MXB Ranked — rank, season stats and the last 50 races.
 *
 * There is nothing to sign into and no account to link. The profile is public and
 * server-rendered, and MX Bikes' GUID is `FF` + the SteamID64, which the app already reads off
 * Steam's own files — so for a Steam copy this tab works the first time it is opened, with no
 * setup at all. A copy bought direct from PiBoSo has a stand-alone GUID only mxb-ranked knows;
 * that one is typed in once, through the same dialog used to look somebody else up.
 *
 * Fetched on open and held for the session. Their site renders each profile on request and is
 * visibly slow about it, so this asks once and then only when told to.
 */
/**
 * The last profile fetched, kept outside the component.
 *
 * Switching tabs unmounts this one, and their site renders every profile on request — slowly,
 * and it starts refusing when asked repeatedly. Coming back to a tab is not a reason to make
 * somebody else's server do that work again, so the answer is held until the app closes or
 * Refresh is pressed.
 */
let cached: RankedProfile | null = null;

const Ranked = () => {
  const t = useT();
  const { reloadConfig } = useConfig();
  const [identity, setIdentity] = useState<RankedIdentity | null>(null);
  const [profile, setProfile] = useState<RankedProfile | null>(cached);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [editing, setEditing] = useState(false);

  // One fetch at a time — their pages take seconds, and two in flight would race to set the
  // same state with no way to tell which answer is current.
  const inFlight = useRef(false);
  const load = useCallback(
    (guid?: string) => {
      if (inFlight.current) return;
      inFlight.current = true;
      setLoading(true);
      setError(null);
      rankedProfile(guid)
        .then((p) => {
          cached = p;
          setProfile(p);
        })
        .catch((e) => {
          const message = typeof e === "string" ? e : String(e);
          setError(message);
          // A refresh that fails on top of a profile we already have must say so out loud:
          // leaving the old numbers up with no word is how you read a stale season as today's.
          if (cached) toast.error(t("ranked.refreshFailed"), { description: message });
        })
        .finally(() => {
          inFlight.current = false;
          setLoading(false);
        });
    },
    [t],
  );

  useEffect(() => {
    rankedIdentity()
      .then((id) => {
        setIdentity(id);
        // Only when there is nothing to show: coming back to the tab reuses what was fetched.
        if (id.guid && !cached) load(id.guid);
      })
      .catch(() => setIdentity({ guid: "", source: "" }));
  }, [load]);

  const saveGuid = useCallback(
    async (guid: string) => {
      try {
        await setRankedGuid(guid);
        await reloadConfig();
        const id = await rankedIdentity();
        setIdentity(id);
        setEditing(false);
        cached = null;
        setProfile(null);
        if (id.guid) load(id.guid);
      } catch (e) {
        toast.error(typeof e === "string" ? e : String(e));
      }
    },
    [load, reloadConfig],
  );

  const open = useCallback(() => {
    if (profile?.url) void openUrl(profile.url);
  }, [profile]);

  return (
    <div className="flex h-full flex-col">
      <ContextBarRight>
        {profile?.season && (
          <span className="text-[12.5px] text-faint">{profile.season}</span>
        )}
        <Button variant="outline" size="sm" onClick={() => setEditing(true)}>
          <Pencil className="size-3.5" />
          {t("ranked.changeGuid")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={() => load(identity?.guid)}
          disabled={loading || !identity?.guid}
        >
          <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
          {t("ranked.refresh")}
        </Button>
        {profile && (
          <Button variant="outline" size="sm" onClick={open}>
            <ExternalLink className="size-3.5" />
            {t("ranked.openSite")}
          </Button>
        )}
      </ContextBarRight>

      <GuidDialog
        open={editing}
        current={identity?.source === "manual" ? identity.guid : ""}
        onClose={() => setEditing(false)}
        onSave={saveGuid}
      />

      <div className="flex min-h-0 flex-1 flex-col px-7 pb-6">
        {identity && !identity.guid ? (
          <Centered>
            <Trophy className="size-6 text-faint" />
            <p className="max-w-[440px] text-center text-[13px] text-muted-foreground">
              {t("ranked.noGuid")}
            </p>
            <Button variant="outline" size="sm" onClick={() => setEditing(true)}>
              {t("ranked.enterGuid")}
            </Button>
          </Centered>
        ) : !profile && loading ? (
          <Centered>
            <Loader2 className="size-5 animate-spin text-faint" />
            <p className="text-[13px] text-faint">{t("ranked.loading")}</p>
          </Centered>
        ) : error && !profile ? (
          <Centered>
            <Trophy className="size-6 text-faint" />
            <p className="max-w-[440px] text-center text-[13px] text-muted-foreground">{error}</p>
            <Button variant="outline" size="sm" onClick={() => load(identity?.guid)}>
              <RefreshCw className="size-3.5" />
              {t("ranked.retry")}
            </Button>
          </Centered>
        ) : profile ? (
          <Profile profile={profile} identity={identity} />
        ) : (
          <Centered>
            <Loader2 className="size-5 animate-spin text-faint" />
          </Centered>
        )}
      </div>
    </div>
  );
};

const Centered = ({ children }: { children: React.ReactNode }) => (
  <div className="flex h-full flex-col items-center justify-center gap-3">{children}</div>
);

/** A two-letter country turned into its flag, or nothing when the site has no country set. */
const flagOf = (code: string) => {
  const c = code.trim().toLowerCase();
  if (c.length !== 2 || c === "un") return "";
  return String.fromCodePoint(
    ...[...c].map((ch) => 0x1f1e6 + (ch.charCodeAt(0) - "a".charCodeAt(0))),
  );
};

/**
 * How much of a rank's own colour to use where.
 *
 * MXB Ranked gives every rank a colour — silver, gold, bronze — and on a screen that is
 * otherwise near-black by design it is the only colour carrying meaning rather than
 * decoration. Used at three strengths: solid on the plate, a line down the card, and a wash
 * faint enough to sit under white text. A rank we get no colour for falls back to the app's
 * own accent, because one grey card beside three coloured ones reads as a fault.
 */
const tint = (color: string, alpha: number) => {
  const c = /^#([0-9a-f]{6})$/i.exec(color.trim());
  if (!c) return `color-mix(in srgb, var(--primary) ${alpha * 100}%, transparent)`;
  const n = parseInt(c[1], 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
};
const solid = (color: string) =>
  /^#([0-9a-f]{6})$/i.test(color.trim()) ? color : "var(--primary)";

/** The behaviour grade, best to worst. Their site colours it; it doesn't tell us with what. */
const RATING_COLOR: Record<string, string> = {
  A: "var(--success)",
  B: "var(--success)",
  C: "var(--warning)",
  D: "var(--destructive)",
  E: "var(--destructive)",
};

/** A podium finish is what you look for in a results list, so it gets the medal's colour. */
const PODIUM = ["#FFD700", "#C0C0C0", "#cd7f32"];
const podiumColor = (position: string) => {
  const place = Number(position.split("/")[0]?.trim());
  return Number.isFinite(place) && place >= 1 && place <= 3 ? PODIUM[place - 1] : "";
};

/**
 * A rank badge as a number plate — the shape the whole app is built from (`.u-skew`, and the
 * cut corner on the cards). A rank is the one number a rider would put on a plate, so it is
 * the one place in the app where the motif is literal rather than decorative.
 */
const Plate = ({
  children,
  color,
  className,
  title,
}: {
  children: React.ReactNode;
  color: string;
  className?: string;
  title?: string;
}) => (
  <span
    title={title}
    className={cn("u-skew grid place-items-center px-2", className)}
    style={{ background: solid(color) }}
  >
    <span className="u-unskew block font-cond font-bold leading-none tracking-[0.04em] text-[#0d1216]">
      {children}
    </span>
  </span>
);

const Profile = ({
  profile,
  identity,
}: {
  profile: RankedProfile;
  identity: RankedIdentity | null;
}) => {
  const t = useT();
  const flag = flagOf(profile.country);
  // The best of the three standings leads: it is the number a rider would give if you asked
  // them what rank they are.
  const headline = profile.cards.find((c) => c.discipline === "Global") ?? profile.cards[0];

  return (
    // Only the results scroll. The rider and their standings stay put, so "how am I doing" is
    // never something you have to scroll back up for.
    <div className="flex min-h-0 flex-1 flex-col gap-3 pt-1">
      <div
        className="u-notch relative shrink-0 overflow-hidden bg-card"
        // A line of the rank's colour along the foot, tying the banner to the cards below.
        style={
          headline ? { boxShadow: `inset 0 -2px 0 ${tint(headline.color, 0.55)}` } : undefined
        }
      >
        {/* The rider's own achievement artwork — earned, different for everyone, and the one
            thing on their profile that isn't a number about them. Behind the text, with a
            wash over it so the name stays readable whatever the picture turns out to be. */}
        {profile.banner && (
          <>
            <CachedImg
              src={profile.banner}
              alt=""
              aria-hidden
              className="absolute inset-0 size-full object-cover"
            />
            {/* Solid behind the name, clearing to the artwork on the right. The picture is
                the point; the gradient only exists so the text on top of it stays readable. */}
            <div className="absolute inset-0 bg-gradient-to-r from-card via-card/85 to-card/20" />
          </>
        )}
        <div className="relative flex min-h-[92px] flex-wrap items-center justify-between gap-4 px-5 py-4">
          <div className="flex items-center gap-4">
            {headline && (
              <Plate color={headline.color} className="h-[46px] w-[58px]" title={headline.rankName}>
                <span className="block text-[19px]">{headline.badge}</span>
                <span className="mt-0.5 block text-[10px] tracking-[0.08em] opacity-75">
                  {headline.mxp}
                </span>
              </Plate>
            )}
            <div>
              <div className="flex items-center gap-2.5">
                {flag && <span className="text-[18px] leading-none">{flag}</span>}
                <h2 className="font-cond text-[26px] font-bold uppercase leading-none tracking-[0.02em]">
                  {profile.name || profile.guid}
                </h2>
                {headline?.rankName && (
                  <span
                    className="font-cond text-[13px] font-semibold uppercase tracking-[0.1em]"
                    style={{ color: solid(headline.color) }}
                  >
                    {headline.rankName}
                  </span>
                )}
              </div>
              <p className="mt-1.5 text-[11.5px] text-faint">
                {profile.guid}
                {profile.memberSince && ` · ${t("ranked.since", { date: profile.memberSince })}`}
                {/* Whose profile this is, because a typed GUID is easy to get subtly wrong and
                    somebody else's season looks exactly like a bad one of your own. */}
                {identity?.source === "manual" && ` · ${t("ranked.manualGuid")}`}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-7 pr-2">
            {profile.exp && <Stat label={t("ranked.exp")} value={profile.exp} />}
            {profile.rating && (
              <Stat
                label={t("ranked.riderRating")}
                value={profile.rating}
                color={RATING_COLOR[profile.rating.toUpperCase()]}
              />
            )}
            {profile.penaltyPoints && (
              <Stat
                label={t("ranked.penaltyPoints")}
                value={profile.penaltyPoints}
                // The number means nothing without the field it is measured against.
                hint={
                  profile.globalPenaltyPoints
                    ? t("ranked.globalAvg", { value: profile.globalPenaltyPoints })
                    : undefined
                }
              />
            )}
          </div>
        </div>
      </div>

      {profile.cards.length > 0 && (
        <div className="grid shrink-0 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {profile.cards.map((c) => (
            <div
              key={c.discipline}
              className="u-notch overflow-hidden bg-card"
              style={{
                // The rank's colour washed off the top-left and drawn down the edge — enough
                // to tell three cards apart across the room, without tinting the surface.
                background: `linear-gradient(135deg, ${tint(c.color, 0.14)} 0%, transparent 55%), var(--card)`,
                boxShadow: `inset 2px 0 0 ${solid(c.color)}`,
              }}
              title={c.rankName}
            >
              <div className="flex items-center justify-between gap-3 px-4 pt-3.5">
                <div>
                  <p className="font-cond text-[17px] font-bold uppercase leading-none tracking-[0.06em]">
                    {c.discipline}
                  </p>
                  <p className="mt-1 text-[11.5px] text-faint">
                    {t("ranked.rank", { rank: c.rank })}
                  </p>
                </div>
                <div className="flex items-center gap-3">
                  <Plate color={c.color} className="h-[30px] w-[38px]">
                    <span className="block text-[13px]">{c.badge}</span>
                  </Plate>
                  <span className="font-cond text-[27px] font-bold leading-none tabular-nums">
                    {c.mxp}
                  </span>
                </div>
              </div>
              <dl className="grid grid-cols-2 gap-x-5 gap-y-1 px-4 pb-3.5 pt-3 text-[12.5px]">
                <Row label={t("ranked.races")} value={c.races} />
                <Row label={t("ranked.avgPosition")} value={c.avgPosition} />
                <Row label={t("ranked.wins")} value={c.wins} />
                <Row label={t("ranked.podiums")} value={c.podiums} />
                <Row label={t("ranked.wrLaps")} value={c.wrLaps} />
                <Row label={t("ranked.pbLaps")} value={c.pbLaps} />
                <Row label={t("ranked.holeshots")} value={c.holeshots} />
              </dl>
            </div>
          ))}
        </div>
      )}

      {profile.races.length > 0 && (
        // `min-h-0` is what actually makes this scroll instead of stretching the page: a flex
        // child refuses to shrink below its content without it.
        <div className="min-h-0 flex-1 overflow-auto border border-input">
          {/* `separate` rather than `collapse`, so the sticky header keeps its own bottom
              border — a collapsed border belongs to the table and scrolls away with it. */}
          <table className="w-full border-separate border-spacing-0 text-[13px]">
            <thead>
              <tr className="text-left text-[11px] uppercase tracking-[0.08em] text-faint">
                {(
                  [
                    ["ranked.track", "px-3.5"],
                    ["ranked.server", "px-2"],
                    ["ranked.position", "w-[76px] px-2"],
                    ["ranked.bike", "px-2"],
                    ["ranked.mxp", "w-[76px] px-2"],
                    ["ranked.penalty", "w-[84px] px-2"],
                    ["ranked.finished", "w-[112px] px-3.5"],
                  ] as const
                ).map(([key, width]) => (
                  <th
                    key={key}
                    className={cn(
                      "sticky top-0 z-10 border-b border-input bg-card py-2.5 font-cond font-semibold",
                      width,
                    )}
                  >
                    {t(key)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {profile.races.map((r, i) => {
                const medal = podiumColor(r.position);
                return (
                  <tr key={`${r.finishedAt}-${i}`} className="group hover:bg-foreground/[0.04]">
                    <td className="max-w-[260px] truncate border-b border-input/50 px-3.5 py-2.5 font-medium group-last:border-0">
                      {/* A bar in the medal's colour, so scanning the list picks out the good
                          days without reading a single number. */}
                      <span
                        className="mr-2.5 inline-block h-3.5 w-[3px] translate-y-[2px] u-skew"
                        style={{ background: medal || "transparent" }}
                      />
                      <span title={r.track}>{r.track}</span>
                    </td>
                    <td
                      className="max-w-[240px] truncate border-b border-input/50 px-2 py-2.5 text-muted-foreground group-last:border-0"
                      title={r.server}
                    >
                      {/* The lobby, without the "MXB-Ranked.com |" every row shares and the
                          "| NA | #102801" nobody reads. */}
                      {lobbyName(r.server)}
                    </td>
                    <td
                      className="border-b border-input/50 px-2 py-2.5 font-cond text-[15px] font-bold tabular-nums group-last:border-0"
                      style={medal ? { color: medal } : undefined}
                    >
                      {r.position}
                    </td>
                    <td
                      className="max-w-[170px] truncate border-b border-input/50 px-2 py-2.5 text-muted-foreground group-last:border-0"
                      title={r.bike}
                    >
                      {r.bike}
                    </td>
                    <td className="border-b border-input/50 px-2 py-2.5 group-last:border-0">
                      <span
                        className={cn(
                          "inline-flex items-center gap-1 tabular-nums",
                          r.mxpDir === "up" && "text-success",
                          r.mxpDir === "down" && "text-destructive",
                          !r.mxpDir && "text-faint",
                        )}
                      >
                        {r.mxpDir === "up" && <ArrowUp className="size-3" />}
                        {r.mxpDir === "down" && <ArrowDown className="size-3" />}
                        {r.mxp}
                      </span>
                    </td>
                    <td className="border-b border-input/50 px-2 py-2.5 tabular-nums text-muted-foreground group-last:border-0">
                      {r.penaltyPoints}
                    </td>
                    <td
                      className="border-b border-input/50 px-3.5 py-2.5 text-faint group-last:border-0"
                      title={r.finishedAt}
                    >
                      {r.finished}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};

/**
 * The lobby's own name out of the full server string.
 *
 * Every row reads "MXB-Ranked.com | MX Freebies | 250 | G+ | NA | #99684": a prefix shared by
 * every row, then the lobby, then a region and a session number. The whole thing stays in the
 * tooltip; the column shows the part that differs.
 */
const lobbyName = (server: string) => {
  const parts = server
    .split("|")
    .map((p) => p.trim())
    .filter((p) => p && !/^#/.test(p) && !/^mxb-ranked\.com$/i.test(p));
  return parts.join(" | ") || server;
};

const Stat = ({
  label,
  value,
  hint,
  color,
}: {
  label: string;
  value: string;
  hint?: string;
  /** Set where the value is a grade rather than a measurement, so it reads at a glance. */
  color?: string;
}) => (
  <div className="text-right" title={hint}>
    <p className="text-[11.5px] uppercase tracking-wide text-faint">{label}</p>
    <p className="text-[17px] font-semibold tabular-nums" style={color ? { color } : undefined}>
      {value}
    </p>
  </div>
);

const Row = ({ label, value }: { label: string; value: string }) =>
  value ? (
    <div className="flex items-center justify-between gap-2">
      <dt className="text-faint">{label}</dt>
      <dd className="tabular-nums">{value}</dd>
    </div>
  ) : null;

export default Ranked;
