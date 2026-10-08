import { describe, expect, test } from "bun:test";
import type { MyMods, PickedFile, UploadJob } from "../src/api/modUpload";
import {
  accountError,
  accountProblem,
  EMPTY_FORM,
  fieldProblems,
  fileProblem,
  MAX_PNT_BYTES,
  MAX_UPLOAD_BYTES,
  modPageUrl,
  needsBike,
  progressOf,
  toMeta,
  type AccountState,
  type UploadForm,
} from "../src/lib/modUpload";

const file = (over: Partial<PickedFile> = {}): PickedFile => ({
  path: "C:/mods/track.pkz",
  filename: "track.pkz",
  size: 1000,
  kind: "pkz",
  ...over,
});

const form = (over: Partial<UploadForm> = {}): UploadForm => ({
  ...EMPTY_FORM,
  type: "tracks",
  title: "Desert MX",
  ...over,
});

const mine = (uploads: MyMods["uploads"] = []): MyMods => ({
  mods: [],
  uploads,
  quota: { openSessions: 3, uploadsPerDay: 20, bytesPerDay: 10 * 1024 ** 3, storageBytes: 25 * 1024 ** 3 },
});

const account = (over: Partial<AccountState> = {}): AccountState => ({
  signedIn: true,
  steam: true,
  blocked: null,
  mine: mine(),
  jobs: [],
  ...over,
});

describe("file checks", () => {
  test("a good file passes", () => {
    expect(fileProblem(file())).toBeNull();
  });
  test("no file, wrong type, empty", () => {
    expect(fileProblem(null)?.code).toBe("noFile");
    expect(fileProblem(file({ filename: "x.rar", kind: null }))?.code).toBe("fileType");
    expect(fileProblem(file({ size: 0 }))?.code).toBe("fileEmpty");
  });
  test("size limits follow the control plane", () => {
    expect(fileProblem(file({ size: MAX_UPLOAD_BYTES }))).toBeNull();
    expect(fileProblem(file({ size: MAX_UPLOAD_BYTES + 1 }))).toEqual({ code: "fileTooBig", max: MAX_UPLOAD_BYTES });
    const pnt = file({ filename: "a.pnt", kind: "pnt" });
    expect(fileProblem({ ...pnt, size: MAX_PNT_BYTES })).toBeNull();
    expect(fileProblem({ ...pnt, size: MAX_PNT_BYTES + 1 })?.code).toBe("fileTooBig");
  });
});

describe("form checks", () => {
  test("a complete new mod passes", () => {
    expect(fieldProblems(form(), file())).toEqual([]);
  });
  test("a new mod needs a type and a title", () => {
    const codes = fieldProblems(form({ type: "", title: "  " }), file()).map((p) => p.code);
    expect(codes).toEqual(["type", "title"]);
  });
  test("paints and liveries need a bike, tracks don't", () => {
    expect(needsBike("paints")).toBe(true);
    expect(needsBike("liveries")).toBe(true);
    expect(needsBike("tracks")).toBe(false);
    expect(fieldProblems(form({ type: "paints" }), file()).map((p) => p.code)).toEqual(["bike"]);
    expect(fieldProblems(form({ type: "paints", bike: "MX1OEM_2023_KTM_450_SX-F" }), file())).toEqual([]);
  });
  test("a .pnt has to be a paint", () => {
    const pnt = file({ filename: "a.pnt", kind: "pnt" });
    expect(fieldProblems(form({ type: "tracks" }), pnt).map((p) => p.code)).toEqual(["pntType"]);
    expect(fieldProblems(form({ type: "paints", bike: "b" }), pnt)).toEqual([]);
  });
  test("length limits", () => {
    const codes = fieldProblems(
      form({ title: "x".repeat(121), description: "x".repeat(5001), version: "x".repeat(41), notes: "x".repeat(2001) }),
      file(),
    ).map((p) => p.code);
    expect(codes).toEqual(["titleLong", "descriptionLong", "versionLong", "notesLong"]);
  });
  test("a new version may leave title and type to the mod", () => {
    expect(fieldProblems(form({ assetId: 7, type: "", title: "" }), file())).toEqual([]);
  });
});

describe("account checks", () => {
  test("ready", () => {
    expect(accountProblem(account())).toBeNull();
  });
  test("blocked wins, then sign-in, then Steam", () => {
    expect(accountProblem(account({ blocked: "nope", signedIn: false, steam: false }))).toEqual({ code: "blocked", message: "nope" });
    expect(accountProblem(account({ signedIn: false, steam: false }))?.code).toBe("signin");
    expect(accountProblem(account({ steam: false }))?.code).toBe("steam");
  });
  test("three open uploads is the limit", () => {
    const open = (id: string) => ({ id, assetId: null, filename: "f", size: 1, state: "open", error: null, createdAt: 0 });
    expect(accountProblem(account({ mine: mine([open("a"), open("b")]) }))).toBeNull();
    expect(accountProblem(account({ mine: mine([open("a"), open("b"), open("c")]) }))).toEqual({ code: "openLimit", limit: 3 });
    const rejected = { ...open("d"), state: "rejected" };
    expect(accountProblem(account({ mine: mine([open("a"), open("b"), rejected]) }))).toBeNull();
  });
  test("the Rust side's refusals", () => {
    expect(accountError("signin")).toEqual({ signedIn: false, blocked: null });
    expect(accountError("blocked:This copy couldn't be verified.")).toEqual({ signedIn: true, blocked: "This copy couldn't be verified." });
    expect(accountError("network:down")).toBeNull();
  });
});

test("toMeta trims and drops what doesn't apply", () => {
  const meta = toMeta(form({ title: " T ", bike: " ", version: " ", notes: "n", description: " d " }));
  expect(meta).toEqual({
    title: "T",
    type: "tracks",
    description: "d",
    bike: "",
    visibility: "public",
    version: null,
    notes: null,
    assetId: null,
  });
  expect(toMeta(form({ assetId: 3, notes: " fixed " })).notes).toBe("fixed");
});

test("progress and links", () => {
  const job = { size: 200, sent: 50, phase: "uploading" } as UploadJob;
  expect(progressOf(job)).toBe(0.25);
  expect(progressOf({ ...job, phase: "checking" })).toBe(1);
  expect(modPageUrl(12)).toBe("https://mxbsecure.com/mods?mod=12");
});
