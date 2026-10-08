import { describe, expect, it } from "vitest";
import worker, { DLQ } from "../index";
import { upsertPost, writeMirrorVersion, type MirrorJob } from "../../src/mirror";
import { d1 } from "../../test/d1sqlite";
import { fakeBucket, fakeQueue } from "../../test/modfakes";

function env() {
  return { DB: d1(), ASSET_MIRROR: fakeBucket(), ASSET_LOCKED: fakeBucket(), MIRROR_QUEUE: fakeQueue() } as unknown as Env;
}

function batch(queue: string, jobs: MirrorJob[]) {
  const acked: MirrorJob[] = [];
  return {
    acked,
    b: {
      queue,
      messages: jobs.map((body) => ({ body, ack: () => void acked.push(body), retry: () => {} })),
    } as unknown as MessageBatch<MirrorJob>,
  };
}

describe("the mirror worker", () => {
  it("serves nobody", async () => {
    const res = await worker.fetch();
    expect(res.status).toBe(404);
  });

  it("puts a dead-lettered file back on the schedule instead of leaving it leased", async () => {
    const e = env();
    await upsertPost(e, { id: 1, slug: "a", link: "https://mxb-mods.com/a/", modified: "2026-01-01T00:00:00" }, new Map(), 0);
    const a = await e.DB.prepare("SELECT id FROM mod_assets").first<{ id: number }>();
    const v = await writeMirrorVersion(e, a!.id, null, [{ url: "https://x/a.zip", host: "x", label: "x", isDefault: true, isServer: false }], 0);
    await e.DB.prepare("UPDATE mod_files SET status = 'queued', leased_until = 999999999999").run();
    const { b, acked } = batch(DLQ, [{ kind: "file", version: v, idx: 0, part: 0 }]);
    await worker.queue(b, e);
    expect(acked).toHaveLength(1);
    expect(await e.DB.prepare("SELECT status, leased_until, error FROM mod_files").first()).toEqual({
      status: "retry",
      leased_until: 0,
      error: "dead-lettered",
    });
  });

  it("acks every job on the main queue, whatever happened to it", async () => {
    const e = env();
    const { b, acked } = batch("mxb-mirror", [{ kind: "file", version: 99, idx: 0, part: 0 }, { kind: "upload", id: "f".repeat(32) }]);
    await worker.queue(b, e);
    expect(acked).toHaveLength(2);
  });
});
