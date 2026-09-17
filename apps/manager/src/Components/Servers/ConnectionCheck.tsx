import { useEffect, useState } from "react";
import {
  Check,
  CircleSlash,
  Loader2,
  Minus,
  RefreshCw,
  ServerOff,
  Stethoscope,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import {
  connectionSelfTest,
  masterStatus,
  type ConnectionCheck as CheckRow,
  type ConnectionSelfTest,
  type MasterStatus,
} from "@frost/shared/api/mods";
import { useT } from "@/i18n";

/**
 * What the app shows instead of a bare error when the server list won't load.
 *
 * MX Bikes answers a dead master server with `connection timeout` and stops, which is the same
 * thing it says for a firewall rule, a broken DNS server and a router that wants restarting.
 * So the failure everyone actually hits is the one failure the game gives you no way to place,
 * and the standing answer in the Discord is a troubleshooting list that sends people to
 * reinstall a game that is working perfectly.
 *
 * One machine cannot answer it. Twenty can: this asks the control plane how many other apps
 * failed the same fetch in the last few minutes and leads with that, because "23 of the last 25
 * apps that tried also failed" ends the question in one line. Only when the crowd is fine, or
 * too quiet to be believed, is it worth asking the player to look at their own machine — and
 * then the self-test does the looking for them rather than handing them a list of things to try.
 */
const ConnectionCheck = ({ error, onRetry }: { error: string; onRetry: () => void }) => {
  const t = useT();
  const [status, setStatus] = useState<MasterStatus | null | undefined>(undefined);
  const [test, setTest] = useState<ConnectionSelfTest | null>(null);
  const [testing, setTesting] = useState(false);

  // Asked as the failure appears, and again whenever the player asks for the list again. Not
  // polled: nobody sits on this screen watching it, and the window behind the answer is ten
  // minutes wide, so a timer would only spend requests on a page that isn't being read.
  const [asked, setAsked] = useState(0);
  useEffect(() => {
    let live = true;
    masterStatus()
      .then((s) => live && setStatus(s))
      .catch(() => live && setStatus(null));
    return () => {
      live = false;
    };
  }, [asked]);

  const retry = () => {
    setStatus(undefined);
    setAsked((n) => n + 1);
    onRetry();
  };

  const runTest = () => {
    setTesting(true);
    connectionSelfTest()
      .then(setTest)
      .catch((e: unknown) => toast.error(typeof e === "string" ? e : String(e)))
      .finally(() => setTesting(false));
  };

  const widespread = status?.state === "down" || status?.state === "degraded";

  return (
    <div className="flex w-full max-w-[540px] flex-col items-center gap-4">
      {widespread ? (
        <CircleSlash className="size-6 text-destructive" />
      ) : (
        <ServerOff className="size-6 text-faint" />
      )}

      <div className="flex flex-col items-center gap-1.5 text-center">
        <p className="text-[13.5px] font-medium">
          {status === undefined
            ? t("connection.checking")
            : widespread
              ? t("connection.notYou")
              : status?.state === "up"
                ? t("connection.likelyYou")
                : t("connection.failed")}
        </p>
        {/* The control plane's own sentence, which carries the counts. Composed there so the
            app, the status page and a Discord bot all say the same thing. */}
        <p className="text-[12.5px] text-muted-foreground">{status?.summary ?? error}</p>
        {widespread && status?.master.failingForMinutes ? (
          <p className="text-[12px] text-faint">
            {t("connection.failingFor", { minutes: status.master.failingForMinutes })}
          </p>
        ) : null}
      </div>

      <div className="flex items-center gap-2">
        <Button variant="outline" size="sm" onClick={retry}>
          <RefreshCw className="size-3.5" />
          {t("serverBrowser.retry")}
        </Button>
        {/* Offered whatever the verdict, because "everyone is fine, so it's you" is the moment
            somebody most wants to know *what* about them, and the app can simply go and look. */}
        <Button variant="ghost" size="sm" onClick={runTest} disabled={testing}>
          {testing ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Stethoscope className="size-3.5" />
          )}
          {t("connection.runCheck")}
        </Button>
      </div>

      {test ? (
        <div className="w-full rounded-lg border border-border/60 bg-popover/40 p-3">
          <p className="mb-2 px-1 text-[12.5px] font-medium">
            {t(`connection.verdict.${test.verdict}` as "connection.verdict.upstream")}
          </p>
          <ul className="flex flex-col">
            {test.checks.map((row) => (
              <CheckLine key={row.id} row={row} />
            ))}
          </ul>
          <p className="mt-2 px-1 text-[11.5px] text-faint">{t("connection.checkFootnote")}</p>
        </div>
      ) : null}
    </div>
  );
};

/** One check. The label is translated; the detail beside it is not — it is for pasting. */
const CheckLine = ({ row }: { row: CheckRow }) => {
  const t = useT();
  const Icon = row.state === "ok" ? Check : row.state === "fail" ? X : Minus;
  return (
    <li className="flex items-baseline gap-2 rounded px-1 py-1 text-[12.5px]">
      <Icon
        className={cn(
          "size-3.5 shrink-0 translate-y-0.5",
          row.state === "ok" && "text-emerald-500",
          row.state === "fail" && "text-destructive",
          row.state === "skip" && "text-faint",
        )}
      />
      <span className="shrink-0">{t(`connection.check.${row.id}` as "connection.check.dns")}</span>
      <span className="truncate text-faint" title={row.detail}>
        {row.detail}
      </span>
    </li>
  );
};

export default ConnectionCheck;
