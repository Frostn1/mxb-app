/**
 * Who may use the FBX → EDF converter on mxbsecure.com, and the route that hands it to them.
 *
 * The converter runs in the visitor's browser as WebAssembly, so the only thing standing
 * between it and anyone at all is who this host will give the files to. Like the locker
 * (`lockweb` in `web.ts`) it therefore lives in R2 rather than on the static site, and is
 * served here, the one host that can see the `__Host-` session cookie.
 *
 * The permission is a list of SteamID64s in `MXB_CONVERTER_STEAM_IDS`, the same shape as
 * `MXB_ADMIN_STEAM_IDS`: granting it is a reviewable diff, and an unset var means nobody but
 * the admins. Admins always have it. A ban takes it away, as it takes away everything else.
 *
 * Only the gate is here. The converter's code is private and never enters this repository —
 * the bucket holds its build, uploaded by the private repo's own deploy.
 */

import { cors } from "./assets";
import { BANNED, isBanned } from "./bans";
import { isSteamId64 } from "./steam";
import { isWebAdmin } from "./webadmin";
import { webSession } from "./websession";

export const CONVERTER_PREFIX = "/v1/web/fbx2edf/";

/** What the site says to a signed-in rider without the permission. */
export const CONVERTER_NOT_GRANTED = "The converter is invite-only for now. Ask us and we'll add you.";

/** The Steam accounts granted the converter, besides the admins. Anything else is dropped. */
export function converterSteamIds(env: Env): string[] {
  return (env.MXB_CONVERTER_STEAM_IDS ?? "").split(/[,\s]+/).filter(isSteamId64);
}

/** Whether this Steam account may use the converter. Says nothing about bans; see the route. */
export function mayConvert(steamId: string, env: Env): boolean {
  return isWebAdmin(steamId, env) || converterSteamIds(env).includes(steamId);
}

/** The files the converter is. A closed list, so a name can never wander out of the bucket. */
const FILES: Record<string, string> = {
  "fbx2edf.js": "text/javascript; charset=utf-8",
  "fbx2edf_bg.wasm": "application/wasm",
};

/** `GET /v1/web/fbx2edf/<file>` — the converter, to a signed-in, granted, unbanned account. */
export async function converterFile(request: Request, url: URL, env: Env, origin: string | null): Promise<Response> {
  const name = url.pathname.slice(CONVERTER_PREFIX.length);
  const type = FILES[name];
  if (!type) return cors(json(404, { error: "no such file" }), origin);

  const session = await webSession(request, env);
  if (!session) return cors(json(401, { error: "sign in with Steam to use the converter" }), origin);
  if (await isBanned(env, { steamId: session.steamId })) return cors(json(403, { error: BANNED }), origin);
  if (!mayConvert(session.steamId, env)) return cors(json(403, { error: CONVERTER_NOT_GRANTED }), origin);

  const object = env.FBX2EDF ? await env.FBX2EDF.get(name) : null;
  // Nothing uploaded, or no bucket bound: a configuration problem, said as 503 so it reads
  // differently from a file that was never servable.
  if (!object) return cors(json(503, { error: "the converter isn't available right now" }), origin);

  return cors(
    new Response(object.body, {
      headers: {
        "Content-Type": type,
        // The visitor's own browser may keep it; no shared cache may — it is not public.
        "Cache-Control": "private, max-age=3600",
        "X-Content-Type-Options": "nosniff",
      },
    }),
    origin,
  );
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}
