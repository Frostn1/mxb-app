/**
 * mxb-mirror: the mod catalogue's slow work, in a Worker of its own.
 *
 * The control plane (`src/index.ts`) answers people: search, mod pages, download redirects,
 * upload sessions, reports and moderation. It never waits on a download host. Everything that
 * does runs here, and nothing here answers a user request:
 *
 *  - the cron: walk mxb-mods.com, queue live-server tracks, evict unused copies (`mirror.ts`,
 *    `mirrorpolicy.ts`), and tidy upload sessions (`uploadcheck.ts`);
 *  - the `mxb-mirror` queue: fetch a mirrored file into R2 (`mirrorfetch.ts`, `mirrorhosts.ts`:
 *    MediaFire, Drive and its virus-scan form, MEGA decryption, folders), or check and publish
 *    an upload from quarantine (`uploadcheck.ts`, `modscan.ts`);
 *  - the `mxb-mirror-dlq` queue: what the consumer gave up on.
 *
 * The code is the control plane's own modules, imported, not copied: one schema, one set of
 * tests. Both Workers bind the same D1 database and the same two buckets; the control plane
 * only produces onto `mxb-mirror`, this Worker consumes it.
 */

import { runMirror, type MirrorJob } from "../src/mirror";
import { consumeMirror, deadLetters } from "../src/mirrorfetch";
import { expireUploads } from "../src/uploadcheck";

export const DLQ = "mxb-mirror-dlq";

export default {
  async scheduled(_event: ScheduledController, env: Env, ctx: ExecutionContext): Promise<void> {
    ctx.waitUntil(Promise.all([runMirror(env), expireUploads(env)]).then(() => undefined));
  },

  async queue(batch: MessageBatch<MirrorJob>, env: Env): Promise<void> {
    if (batch.queue === DLQ) return deadLetters(batch, env);
    await consumeMirror(batch, env);
  },

  /** Not a public service: no route, and nothing to say to anyone who finds its address. */
  async fetch(): Promise<Response> {
    return new Response("not found", { status: 404 });
  },
} satisfies ExportedHandler<Env, MirrorJob>;
