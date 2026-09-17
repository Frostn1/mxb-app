import { describe, expect, it } from "vitest";

import {
  clearNote,
  collectSurvey,
  deletePoll,
  listPolls,
  MAX_ANSWERS_PER_DAY,
  MAX_NOTES,
  NOTE_RETENTION_DAYS,
  parseAnswer,
  parsePoll,
  pruneSurvey,
  reportAnswer,
  RETENTION_DAYS,
  savePoll,
  scrubNote,
  setPollLive,
  surveyStats,
  windowDays,
  windowPoll,
  type Poll,
} from "../src/survey";
import { MAX_NOTE_CHARS, MAX_REASONS_PER_ANSWER } from "../src/validate";
import { d1 } from "./d1sqlite";

const INSTALL = "6f1f2b6c-0f6d-4a5e-9f3a-2b7c4d5e6f70";
const OTHER = "11112222-3333-4444-8555-666677778888";

/** An answer the endpoint would accept, so each test can spoil one thing about it. */
function body(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    installId: INSTALL,
    app: "manager",
    version: "0.15.1",
    os: "windows",
    game: "mxb",
    pollId: "mood",
    choice: "good",
    ...over,
  };
}

function post(payload: unknown, headers: Record<string, string> = {}): Request {
  return new Request("https://cp.test/v1/survey", {
    method: "POST",
    headers: { "content-type": "application/json", ...headers },
    body: typeof payload === "string" ? payload : JSON.stringify(payload),
  });
}

function env(db: Env["DB"], over: Record<string, unknown> = {}): Env {
  return { DB: db, ...over } as unknown as Env;
}

/** A poll the form would accept. */
function poll(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: "race-mode",
    kind: "choice",
    apps: ["manager"],
    ask: { en: "Tried Race mode yet?" },
    choices: [
      { id: "yes", label: { en: "Yes" } },
      { id: "no", label: { en: "Not yet" } },
    ],
    ...over,
  };
}

describe("an answer", () => {
  it("is a poll, a choice and nothing that identifies anyone", () => {
    const parsed = parseAnswer(JSON.stringify(body({ reasons: ["crash", "slow"] })));
    expect(typeof parsed).not.toBe("string");
    if (typeof parsed === "string") return;
    expect(parsed.pollId).toBe("mood");
    expect(parsed.choice).toBe("good");
    expect(parsed.reasons).toEqual(["crash", "slow"]);
    expect(parsed.note).toBeNull();
  });

  it("needs an install id that is a UUID", () => {
    expect(parseAnswer(JSON.stringify(body({ installId: "frost" })))).toBe("installId must be a UUID");
  });

  it("refuses a choice that isn't a slug, which is what a rider name would arrive as", () => {
    expect(parseAnswer(JSON.stringify(body({ choice: "Frost Rider" })))).toContain("choice");
  });

  it("refuses a poll id that isn't one", () => {
    expect(parseAnswer(JSON.stringify(body({ pollId: "../../etc" })))).toContain("pollId");
  });

  it("folds a chip sent twice rather than refusing the answer", () => {
    const parsed = parseAnswer(JSON.stringify(body({ reasons: ["crash", "crash"] })));
    if (typeof parsed === "string") throw new Error(parsed);
    expect(parsed.reasons).toEqual(["crash"]);
  });

  it("refuses more chips than a question could have", () => {
    const many = Array.from({ length: MAX_REASONS_PER_ANSWER + 1 }, (_, i) => `r${i}`);
    expect(parseAnswer(JSON.stringify(body({ reasons: many })))).toContain("too many reasons");
  });

  it("must say which app asked", () => {
    expect(parseAnswer(JSON.stringify(body({ app: "overlay" })))).toContain("app must be");
  });
});

describe("a note", () => {
  it("keeps the sentence and loses the address", () => {
    expect(scrubNote("it crashes, mail me at frost@example.com")).toBe(
      "it crashes, mail me at [email]",
    );
  });

  it("loses a link", () => {
    expect(scrubNote("see https://imgur.com/a/abc123 for a shot")).toBe("see [link] for a shot");
  });

  it("loses the path that carries somebody's name", () => {
    expect(scrubNote("fails at C:\\Users\\Jamie\\Documents\\mods")).toBe("fails at [path]");
    expect(scrubNote("nothing under /home/jamie/.mxb")).toBe("nothing under [path]");
  });

  it("is one line, whatever was typed", () => {
    expect(scrubNote("one\ntwo\r\n\tthree")).toBe("one two three");
  });

  it("is capped", () => {
    const long = scrubNote("x".repeat(MAX_NOTE_CHARS * 2));
    expect(long).not.toBeNull();
    expect(long!.length).toBeLessThanOrEqual(MAX_NOTE_CHARS);
  });

  it("is null when there was nothing in the box", () => {
    expect(scrubNote("   ")).toBeNull();
    expect(scrubNote(undefined)).toBeNull();
    expect(scrubNote(42)).toBeNull();
  });
});

describe("posting an answer", () => {
  it("stores it, and a second post replaces rather than votes twice", async () => {
    const db = d1();
    expect((await reportAnswer(post(body()), env(db))).status).toBe(202);
    expect((await reportAnswer(post(body({ choice: "fine" })), env(db))).status).toBe(202);

    const rows = await db.prepare("SELECT choice FROM survey_answers").all<{ choice: string }>();
    expect(rows.results).toEqual([{ choice: "fine" }]);
  });

  it("never blanks a note a later post didn't carry", async () => {
    const db = d1();
    await reportAnswer(post(body({ choice: "bad", note: "it froze on launch" })), env(db));
    await reportAnswer(post(body({ choice: "bad", reasons: ["crash"] })), env(db));

    const row = await db
      .prepare("SELECT note, reasons FROM survey_answers")
      .first<{ note: string; reasons: string }>();
    expect(row?.note).toBe("it froze on launch");
    expect(row?.reasons).toBe("crash");
  });

  it("insists on JSON, so a web page can't post from its visitors", async () => {
    const req = new Request("https://cp.test/v1/survey", {
      method: "POST",
      headers: { "content-type": "text/plain" },
      body: JSON.stringify(body()),
    });
    expect((await reportAnswer(req, env(d1()))).status).toBe(415);
  });

  it("refuses a body far too big to be an answer", async () => {
    const req = post(body(), { "content-length": String(64 * 1024) });
    expect((await reportAnswer(req, env(d1()))).status).toBe(413);
  });

  it("is refused unsigned once the deployment requires a signature", async () => {
    const db = d1();
    const strict = env(db, { MXB_USAGE_REQUIRE_SIGNATURE: "1", USAGE_SIGNING_KEY: "k" });
    expect((await reportAnswer(post(body()), strict)).status).toBe(401);

    // And accepted with the signature the app's own build key produces.
    const raw = JSON.stringify(body());
    const seconds = Math.floor(Date.now() / 1000);
    const key = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode("k"),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const mac = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(`v1.${seconds}.${raw}`));
    const hex = [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, "0")).join("");
    const signed = post(raw, { "X-MXB-Usage": `v1 ${seconds} ${hex}` });
    expect((await reportAnswer(signed, strict)).status).toBe(202);
  });

  it("turns an address away once it has answered all day", async () => {
    const db = d1();
    await db
      .prepare(
        "INSERT INTO device_claims (ip_digest, day, kind, claims, updated_at) VALUES (?, ?, 'survey', ?, ?)",
      )
      .bind(
        await digest(),
        new Date().toISOString().slice(0, 10),
        MAX_ANSWERS_PER_DAY,
        Date.now(),
      )
      .run();
    expect((await reportAnswer(post(body()), env(db))).status).toBe(429);
  });
});

describe("the questions", () => {
  it("ships with the standing one, live and asking every app", async () => {
    const res = await listPolls(new URL("https://cp.test/v1/survey/polls"), env(d1()));
    const { polls } = (await res.json()) as { polls: Poll[] };
    const mood = polls.find((p) => p.id === "mood");
    expect(mood?.kind).toBe("mood");
    expect(mood?.apps).toEqual(["manager", "studio", "coach"]);
    expect(mood?.followUp).toEqual(["bad"]);
    // The point of the column: asked again, but nothing like daily.
    expect(mood?.againDays).toBeGreaterThan(7);
  });

  it("only hands an app the questions meant for it", async () => {
    const db = d1();
    const written = parsePoll(poll({ apps: ["studio"] }));
    if (typeof written === "string") throw new Error(written);
    await savePoll(env(db), written);

    const forCoach = await listPolls(new URL("https://cp.test/v1/survey/polls?app=coach"), env(db));
    const { polls } = (await forCoach.json()) as { polls: Poll[] };
    expect(polls.map((p) => p.id)).toEqual(["mood"]);
  });

  it("leaves out one whose window hasn't opened, or has closed", async () => {
    const db = d1();
    const now = Date.now();
    const soon = parsePoll(poll({ id: "soon", startsAt: now + 86_400_000 }), now);
    if (typeof soon === "string") throw new Error(soon);
    await savePoll(env(db), soon);

    const res = await listPolls(new URL("https://cp.test/v1/survey/polls"), env(db), now);
    const { polls } = (await res.json()) as { polls: Poll[] };
    expect(polls.map((p) => p.id)).toEqual(["mood"]);
  });

  it("is cacheable, because it is the same for everybody", async () => {
    const res = await listPolls(new URL("https://cp.test/v1/survey/polls"), env(d1()));
    expect(res.headers.get("Cache-Control")).toContain("max-age=");
  });

  it("is retired rather than deleted once anyone has answered", async () => {
    const db = d1();
    await reportAnswer(post(body()), env(db));
    expect(await deletePoll(env(db), "mood")).toBe("answered");
    expect(await setPollLive(env(db), "mood", false)).toBe(true);

    const res = await listPolls(new URL("https://cp.test/v1/survey/polls"), env(db));
    const { polls } = (await res.json()) as { polls: Poll[] };
    expect(polls).toEqual([]);
  });
});

describe("writing a question", () => {
  it("needs a slug for an id", () => {
    expect(parsePoll(poll({ id: "Race Mode" }))).toContain("id must be a slug");
  });

  it("needs something to ask", () => {
    expect(parsePoll(poll({ ask: {} }))).toContain("English");
  });

  it("needs at least two answers", () => {
    expect(parsePoll(poll({ choices: [{ id: "yes", label: { en: "Yes" } }] }))).toContain(
      "at least two answers",
    );
  });

  it("refuses two answers under one id, which would add up as one", () => {
    const twice = poll({
      choices: [
        { id: "yes", label: { en: "Yes" } },
        { id: "yes", label: { en: "Also yes" } },
      ],
    });
    expect(parsePoll(twice)).toContain("twice");
  });

  it("refuses a follow-up on an answer the question doesn't have", () => {
    expect(parsePoll(poll({ followUp: ["maybe"] }))).toContain("not one of the answers");
  });

  it("lets the mood poll name its own follow-up answers, which the app draws", () => {
    const mood = parsePoll({ id: "mood", kind: "mood", apps: ["manager"], followUp: ["bad"] });
    expect(typeof mood).not.toBe("string");
  });

  it("won't let the mood poll invent answers the app can't draw", () => {
    const wrong = parsePoll({
      id: "mood",
      kind: "mood",
      apps: ["manager"],
      choices: [{ id: "awful", label: { en: "Awful" } }],
    });
    expect(wrong).toContain("draws its own answers");
  });

  it("refuses a window that has already closed", () => {
    expect(parsePoll(poll({ endsAt: Date.now() - 1000 }))).toContain("retire");
  });

  it("refuses a follow-up chance outside 0 to 1", () => {
    expect(parsePoll(poll({ followUpChance: 4 }))).toContain("0 to 1");
  });

  it("round-trips through the database", async () => {
    const db = d1();
    const written = parsePoll(
      poll({ reasons: [{ id: "slow", label: { en: "Too slow", it: "Troppo lento" } }], note: false }),
    );
    if (typeof written === "string") throw new Error(written);
    await savePoll(env(db), written);

    const res = await listPolls(new URL("https://cp.test/v1/survey/polls?app=manager"), env(db));
    const { polls } = (await res.json()) as { polls: Poll[] };
    const back = polls.find((p) => p.id === "race-mode");
    expect(back?.ask.en).toBe("Tried Race mode yet?");
    expect(back?.reasons[0].label.it).toBe("Troppo lento");
    expect(back?.note).toBe(false);
  });
});

describe("reading the answers", () => {
  it("counts each choice, and scores the mood poll", async () => {
    const db = d1();
    await reportAnswer(post(body({ choice: "good" })), env(db));
    await reportAnswer(post(body({ installId: OTHER, choice: "bad", reasons: ["crash"] })), env(db));

    const stats = await collectSurvey(env(db), 30);
    expect(stats.answers).toBe(2);
    expect(stats.installs).toBe(2);
    expect(stats.choices.find((c) => c.id === "good")?.answers).toBe(1);
    expect(stats.reasons).toEqual([{ id: "crash", answers: 1 }]);
    // One good, one bad: they cancel.
    expect(stats.score).toBe(0);
  });

  it("scores nothing for a question whose answers have no order", async () => {
    const db = d1();
    const written = parsePoll(poll());
    if (typeof written === "string") throw new Error(written);
    await savePoll(env(db), written);
    await reportAnswer(post(body({ pollId: "race-mode", choice: "yes" })), env(db));

    const stats = await collectSurvey(env(db), 30, Date.now(), "all", "race-mode");
    expect(stats.answers).toBe(1);
    expect(stats.score).toBeNull();
  });

  it("narrows to one app without the others leaking in", async () => {
    const db = d1();
    await reportAnswer(post(body({ app: "manager", choice: "good" })), env(db));
    await reportAnswer(post(body({ installId: OTHER, app: "coach", choice: "bad" })), env(db));

    const coach = await collectSurvey(env(db), 30, Date.now(), "coach");
    expect(coach.answers).toBe(1);
    expect(coach.choices).toEqual([{ id: "bad", answers: 1, installs: 1 }]);
  });

  it("names a retired poll, so its answers can still be read", async () => {
    const db = d1();
    await reportAnswer(post(body()), env(db));
    await setPollLive(env(db), "mood", false);

    const stats = await collectSurvey(env(db), 30);
    expect(stats.polls.map((p) => p.id)).toContain("mood");
  });

  it("hands back the notes with a handle that deletes one", async () => {
    const db = d1();
    await reportAnswer(post(body({ choice: "bad", note: "froze on launch" })), env(db));

    const before = await collectSurvey(env(db), 30);
    expect(before.notes).toHaveLength(1);
    expect(before.notes[0].note).toBe("froze on launch");

    expect(await clearNote(env(db), before.notes[0].handle)).toBe(true);
    const after = await collectSurvey(env(db), 30);
    expect(after.notes).toHaveLength(0);
    // The answer itself is untouched — the choice stays counted.
    expect(after.answers).toBe(1);
  });

  it("refuses a handle that isn't one", async () => {
    const db = d1();
    expect(await clearNote(env(db), "mood|not-a-uuid|2026-09-17")).toBe(false);
    expect(await clearNote(env(db), 7)).toBe(false);
  });

  it("is behind the admin key, like every other read of everybody's numbers", async () => {
    const db = d1();
    const url = new URL("https://cp.test/v1/survey/stats");
    const plain = new Request(url);
    expect((await surveyStats(plain, url, env(db))).status).toBe(503);
    expect((await surveyStats(plain, url, env(db, { ADMIN_KEY: "s3cret" }))).status).toBe(401);

    const keyed = new Request(url, { headers: { Authorization: "Bearer s3cret" } });
    expect((await surveyStats(keyed, url, env(db, { ADMIN_KEY: "s3cret" }))).status).toBe(200);
  });
});

describe("the sweep", () => {
  it("drops the note long before the answer it came with", async () => {
    const db = d1();
    const old = (back: number) => new Date(Date.now() - back * 86_400_000).toISOString().slice(0, 10);
    const row = (install: string, day: string) =>
      db
        .prepare(
          "INSERT INTO survey_answers (poll_id, install_id, app, day, version, os, game, choice, reasons, note, answered_at)" +
            " VALUES ('mood', ?, 'manager', ?, '0.15.1', 'windows', 'mxb', 'bad', '', 'something', 0)",
        )
        .bind(install, day)
        .run();

    await row(INSTALL, old(NOTE_RETENTION_DAYS + 1));
    await row(OTHER, old(RETENTION_DAYS + 1));
    await row("22223333-4444-5555-8666-777788889999", old(1));
    await pruneSurvey(env(db));

    const rows = await db
      .prepare("SELECT day, note FROM survey_answers ORDER BY day")
      .all<{ day: string; note: string | null }>();
    // The oldest is gone entirely; the middle one keeps its row and loses its note; today's is
    // untouched.
    expect(rows.results).toHaveLength(2);
    expect(rows.results?.[0].note).toBeNull();
    expect(rows.results?.[1].note).toBe("something");
  });
});

describe("what a read may ask for", () => {
  it("clamps the window and defaults the poll", () => {
    expect(windowDays(new URL("https://cp.test/x?days=9999"))).toBe(365);
    expect(windowDays(new URL("https://cp.test/x?days=nope"))).toBe(30);
    expect(windowPoll(new URL("https://cp.test/x"))).toBe("mood");
    expect(windowPoll(new URL("https://cp.test/x?poll=Race%20Mode"))).toBe("mood");
    expect(windowPoll(new URL("https://cp.test/x?poll=race-mode"))).toBe("race-mode");
  });

  it("never hands back more notes than a page is for reading", () => {
    expect(MAX_NOTES).toBeLessThanOrEqual(500);
  });
});

/** The per-address key the endpoint computes, so a test can pre-fill its bucket. */
async function digest(): Promise<string> {
  const day = new Date().toISOString().slice(0, 10);
  const material = new TextEncoder().encode(`${day}:unknown`);
  const bytes = new Uint8Array(await crypto.subtle.digest("SHA-256", material));
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}
