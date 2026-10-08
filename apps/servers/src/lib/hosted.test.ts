import { describe, expect, it } from "vitest";
import { claimCode, hostedReport, type HostedServer } from "./hosted";

const CODE = "AbCdEfGhIjKlMnOp_-123456";

describe("claimCode", () => {
  it("takes a bare code or the code out of a whole link", () => {
    expect(claimCode(`  ${CODE} `)).toBe(CODE);
    expect(claimCode(`mxbservers://hosted/claim?code=${CODE}`)).toBe(CODE);
    expect(claimCode(`mxbservers://hosted/claim/?ref=x&code=${CODE}`)).toBe(CODE);
    expect(claimCode(`mxbservers://connect?claim=${CODE}`)).toBe(CODE);
  });
});

const server = (over: Partial<HostedServer>): HostedServer => ({
  id: "0f8fad5b-d9cb-469f-a165-70867728950e",
  name: "Friday",
  type: "mxbserver",
  region: "eu-west",
  regionLabel: "EU West",
  state: "ready",
  progress: { step: 3, steps: ["Provisioning", "Installing", "Ready"], since: 0, note: null },
  address: "51.81.10.18:54210",
  settings: { track: "club", bikeSet: "oem-mx2", maxRiders: 20 },
  options: { tracks: [], bikeSets: [], maxRiders: 20 },
  riders: 0,
  idleSince: null,
  freedAt: null,
  createdAt: 0,
  error: null,
  ...over,
});

describe("hostedReport", () => {
  it("speaks the same states as every other server", () => {
    expect(hostedReport(server({})).state).toBe("online");
    expect(hostedReport(server({ state: "installing", progress: { step: 2, steps: ["Provisioning", "Installing", "Ready"], since: 0, note: null } }))).toMatchObject({ state: "starting", detail: "Installing" });
    expect(hostedReport(server({ state: "failed", error: "boom" }))).toMatchObject({ state: "unreachable", detail: "boom" });
  });
});
