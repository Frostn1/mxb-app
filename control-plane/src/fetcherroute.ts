/**
 * Which hosts the mirror reaches through the fetcher on our own box instead of from a Worker.
 *
 * Some sites answer Cloudflare Workers 403 and a normal machine 200: mxb-mods.com and MediaFire
 * both do. `MIRROR_FETCHER_HOSTS` (a var in both wrangler files) names the hosts that always go
 * via the fetcher, comma or space separated; a name covers its subdomains. Empty (or unset)
 * means no fetcher: everything runs in the Worker as before.
 *
 * With a fetcher configured, a host that refuses the Worker a file (401/403) is also learnt
 * (`mirror_state` key `fetcher_hosts`) and its files go to the fetcher from then on.
 */

const LEARNT_KEY = "fetcher_hosts";
/** A cap on learnt hosts, so a run of odd links can't grow the row without bound. */
const MAX_LEARNT = 200;

export function hostname(url: string): string {
  try {
    return new URL(url).hostname.toLowerCase().replace(/^www\./, "");
  } catch {
    return "";
  }
}

export function configuredFetcherHosts(env: Env): string[] {
  return (env.MIRROR_FETCHER_HOSTS ?? "")
    .split(/[\s,]+/)
    .map((h) => h.trim().toLowerCase().replace(/^www\./, ""))
    .filter(Boolean);
}

/** Is there a fetcher at all? */
export function fetcherOn(env: Env): boolean {
  return configuredFetcherHosts(env).length > 0;
}

export function hostIn(host: string, list: string[]): boolean {
  return !!host && list.some((h) => host === h || host.endsWith(`.${h}`));
}

/** The configured hosts plus the learnt ones. */
async function allHosts(env: Env): Promise<string[]> {
  const conf = configuredFetcherHosts(env);
  if (conf.length === 0) return [];
  const row = await env.DB.prepare("SELECT value FROM mirror_state WHERE key = ?").bind(LEARNT_KEY).first<{ value: string }>();
  let learnt: string[] = [];
  try {
    learnt = row ? (JSON.parse(row.value) as string[]) : [];
  } catch {
    learnt = [];
  }
  return [...conf, ...learnt];
}

/** A test for "does this URL go via the fetcher", loaded once for a batch of rows. */
export async function fetcherRouter(env: Env): Promise<(url: string) => boolean> {
  const hosts = await allHosts(env);
  return (url) => hostIn(hostname(url), hosts);
}

/** Do mxb-mods.com's pages go via the fetcher? */
export function pagesViaFetcher(env: Env): boolean {
  return hostIn("mxb-mods.com", configuredFetcherHosts(env));
}

/** Remember that this URL's host refuses the Worker. A no-op without a fetcher. */
export async function learnFetcherHost(env: Env, url: string): Promise<void> {
  const host = hostname(url);
  if (!host || !fetcherOn(env)) return;
  const hosts = await allHosts(env);
  if (hostIn(host, hosts)) return;
  const learnt = hosts.slice(configuredFetcherHosts(env).length);
  if (learnt.length >= MAX_LEARNT) return;
  await env.DB.prepare(
    "INSERT INTO mirror_state (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
  )
    .bind(LEARNT_KEY, JSON.stringify([...learnt, host]))
    .run();
  console.warn(JSON.stringify({ msg: "mirror host refuses the Worker; routed to the fetcher", host }));
}
