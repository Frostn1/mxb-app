import { useCallback, useEffect, useRef, useState } from "react";
import { ExternalLink, Loader2, RefreshCw, Trophy, ArrowUp, ArrowDown, Pencil } from "lucide-react";
import { toast } from "sonner";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { cn } from "@/lib/utils";
import { Button } from "@/Components/ui/button";
import { ContextBarRight } from "../Shell/ContextBar";
import {
  rankedIdentity,
  rankedProfile,
  setRankedGuid,
  type RankedProfile,
  type RankedIdentity,
} from "../../api/ranked";
import { useConfig } from "../../Context/Config";
import { useT } from "../../i18n/context";
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

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
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

const Profile = ({
  profile,
  identity,
}: {
  profile: RankedProfile;
  identity: RankedIdentity | null;
}) => {
  const t = useT();
  const flag = flagOf(profile.country);

  return (
    <div className="flex flex-col gap-4 pt-1">
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-input bg-card px-4 py-3">
        <div className="flex items-center gap-3">
          {flag && <span className="text-[20px] leading-none">{flag}</span>}
          <div>
            <h2 className="text-[16px] font-semibold">{profile.name || profile.guid}</h2>
            <p className="text-[11.5px] text-faint">
              {profile.guid}
              {profile.memberSince && ` · ${t("ranked.since", { date: profile.memberSince })}`}
              {/* Whose profile this is, because a typed GUID is easy to get subtly wrong and
                  somebody else's season looks exactly like a bad one of your own. */}
              {identity?.source === "manual" && ` · ${t("ranked.manualGuid")}`}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-5">
          {profile.exp && (
            <Stat label={t("ranked.exp")} value={profile.exp} />
          )}
          {profile.rating && (
            <Stat label={t("ranked.riderRating")} value={profile.rating} />
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

      {profile.cards.length > 0 && (
        <div className="flex flex-wrap gap-3">
          {profile.cards.map((c) => (
            <div
              key={c.discipline}
              className="min-w-[240px] flex-1 overflow-hidden rounded-xl border border-input"
            >
              <div
                className="flex items-center justify-between px-3.5 py-2.5 text-black"
                // The rank's own colour, straight off their card, so the two read as the
                // same thing rather than as our idea of what Silver looks like.
                style={c.color ? { background: c.color } : undefined}
                title={c.rankName}
              >
                <div>
                  <p className="text-[13px] font-semibold">{c.discipline}</p>
                  <p className="text-[11.5px] opacity-80">
                    {t("ranked.rank", { rank: c.rank })}
                  </p>
                </div>
                <div className="flex items-center gap-2.5">
                  <span className="border border-black/20 px-1.5 py-px text-[11.5px] font-semibold">
                    {c.badge}
                  </span>
                  <span className="text-[17px] font-semibold tabular-nums">{c.mxp}</span>
                </div>
              </div>
              <dl className="grid grid-cols-2 gap-x-4 gap-y-1 bg-card px-3.5 py-2.5 text-[12.5px]">
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
        <div className="overflow-hidden rounded-xl border border-input">
          <table className="w-full border-collapse text-[13px]">
            <thead>
              <tr className="border-b border-input bg-card text-left text-[11.5px] uppercase tracking-wide text-faint">
                <th className="px-3.5 py-2.5 font-semibold">{t("ranked.track")}</th>
                <th className="px-2 py-2.5 font-semibold">{t("ranked.server")}</th>
                <th className="w-[70px] px-2 py-2.5 font-semibold">{t("ranked.position")}</th>
                <th className="px-2 py-2.5 font-semibold">{t("ranked.bike")}</th>
                <th className="w-[70px] px-2 py-2.5 font-semibold">{t("ranked.mxp")}</th>
                <th className="w-[80px] px-2 py-2.5 font-semibold">{t("ranked.penalty")}</th>
                <th className="w-[110px] px-3.5 py-2.5 font-semibold">{t("ranked.finished")}</th>
              </tr>
            </thead>
            <tbody>
              {profile.races.map((r, i) => (
                <tr
                  key={`${r.finishedAt}-${i}`}
                  className="border-b border-input/60 last:border-0 hover:bg-foreground/[0.03]"
                >
                  <td className="max-w-[260px] truncate px-3.5 py-2.5 font-medium" title={r.track}>
                    {r.track}
                  </td>
                  <td className="max-w-[280px] truncate px-2 py-2.5 text-muted-foreground" title={r.server}>
                    {r.server}
                  </td>
                  <td className="px-2 py-2.5 tabular-nums">{r.position}</td>
                  <td className="max-w-[180px] truncate px-2 py-2.5 text-muted-foreground" title={r.bike}>
                    {r.bike}
                  </td>
                  <td className="px-2 py-2.5">
                    <span
                      className={cn(
                        "inline-flex items-center gap-1 tabular-nums",
                        r.mxpDir === "up" && "text-success",
                        r.mxpDir === "down" && "text-destructive",
                      )}
                    >
                      {r.mxpDir === "up" && <ArrowUp className="size-3" />}
                      {r.mxpDir === "down" && <ArrowDown className="size-3" />}
                      {r.mxp}
                    </span>
                  </td>
                  <td className="px-2 py-2.5 tabular-nums text-muted-foreground">
                    {r.penaltyPoints}
                  </td>
                  <td className="px-3.5 py-2.5 text-faint" title={r.finishedAt}>
                    {r.finished}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};

const Stat = ({ label, value, hint }: { label: string; value: string; hint?: string }) => (
  <div className="text-right" title={hint}>
    <p className="text-[11.5px] uppercase tracking-wide text-faint">{label}</p>
    <p className="text-[15px] font-semibold tabular-nums">{value}</p>
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
