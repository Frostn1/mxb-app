import { Lock, Plug, Loader2, Signal, Users } from "lucide-react";
import { Button } from "@/Components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/Components/ui/dialog";
import { useT } from "../../i18n/context";
import type { MasterServer } from "../../api/mods";

/**
 * Everything one server publishes about itself.
 *
 * The list row stays to what a player scans for — who's on, how far away, which track. The
 * rest of it is real data the master has always carried and we used to throw away: the
 * event blob holds the track, the session and the rules, and `location` and the licence
 * class sit beside it. It lands here rather than in the row because it is what you read
 * once, before deciding to join, not what you compare fifty rows on.
 */

/** One label/value line. Values that came back empty are dropped by {@link Facts}. */
type Fact = { label: string; value: string };

const Facts = ({ title, facts }: { title: string; facts: Fact[] }) => {
  const shown = facts.filter((f) => f.value);
  if (shown.length === 0) return null;
  return (
    <section className="space-y-2">
      <h3 className="text-[11.5px] font-semibold uppercase tracking-wide text-faint">
        {title}
      </h3>
      <dl className="grid grid-cols-[minmax(0,7rem)_1fr] gap-x-4 gap-y-1.5 text-[13px]">
        {shown.map((f) => (
          <div key={f.label} className="contents">
            <dt className="truncate text-muted-foreground">{f.label}</dt>
            <dd className="min-w-0 break-words">{f.value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
};

const ServerDetail = ({
  server,
  onOpenChange,
  onJoin,
  joining,
}: {
  server: MasterServer | null;
  onOpenChange: (open: boolean) => void;
  onJoin: (address: string) => void;
  joining: string | null;
}) => {
  const t = useT();
  if (!server) return null;
  const s = server;

  const yes = t("serverBrowser.yes");
  const flag = (on: boolean) => (on ? yes : "");
  // An empty allow-list is the server saying it doesn't mind, which is worth stating.
  const list = (xs: string[]) => (xs.length ? xs.join(", ") : t("serverBrowser.any"));

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[560px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 pr-6">
            {s.passworded && <Lock className="size-4 shrink-0 text-faint" />}
            <span className="truncate">{s.name}</span>
          </DialogTitle>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[12.5px] text-muted-foreground">
            <span className="inline-flex items-center gap-1.5">
              <Users className="size-3.5 text-faint" />
              {s.players}/{s.maxPlayers}
            </span>
            {s.pingMs !== null && (
              <span className="inline-flex items-center gap-1.5 tabular-nums">
                <Signal className="size-3.5 text-faint" />
                {s.pingMs} ms
              </span>
            )}
            {s.location && <span>{s.location}</span>}
          </div>
        </DialogHeader>

        <div className="max-h-[60vh] space-y-5 overflow-y-auto pr-1">
          <Facts
            title={t("serverBrowser.running")}
            facts={[
              { label: t("servers.track"), value: s.track },
              { label: t("serverBrowser.layout"), value: s.trackLayout },
              { label: t("serverBrowser.session"), value: s.session },
              { label: t("serverBrowser.raceLength"), value: s.raceLength },
              { label: t("serverBrowser.conditions"), value: s.conditions },
              {
                label: t("serverBrowser.changingWeather"),
                value: flag(s.realisticWeather),
              },
            ]}
          />

          <Facts
            title={t("serverBrowser.rules")}
            facts={[
              { label: t("serverBrowser.categories"), value: list(s.categories) },
              { label: t("serverBrowser.bikes"), value: list(s.bikes) },
              { label: t("serverBrowser.rating"), value: s.rating },
              { label: t("serverBrowser.forceCockpit"), value: flag(s.forceCockpit) },
              { label: t("serverBrowser.noAids"), value: flag(s.noAids) },
              {
                label: t("serverBrowser.limitedTyres"),
                value: flag(s.limitedTyreSets),
              },
              {
                label: t("serverBrowser.passworded"),
                value: flag(s.passworded),
              },
            ]}
          />

          <Facts
            title={t("serverBrowser.connection")}
            facts={[
              { label: t("serverBrowser.address"), value: s.address },
              { label: t("serverBrowser.lanAddress"), value: s.lanAddress },
            ]}
          />
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-input pt-3">
          {/* A server the game can't be pointed at says so, rather than offering a button
              that fails every time. */}
          <p className="text-[12px] text-faint">
            {s.joinable ? "" : t("serverBrowser.notJoinable")}
          </p>
          <Button
            onClick={() => onJoin(s.address)}
            disabled={!s.joinable || joining !== null}
          >
            {joining === s.address ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Plug className="size-3.5" />
            )}
            {t("serverBrowser.join")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
};

export default ServerDetail;
