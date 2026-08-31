import { describe, expect, it } from "vitest";
import {
  isVerified,
  loginUrl,
  returnToMatches,
  steamIdFromClaimedId,
  verifyAssertion,
} from "../src/steam";

/** A well-formed assertion, as Steam sends one back. */
function assertion(over: Record<string, string> = {}): URLSearchParams {
  return new URLSearchParams({
    "openid.ns": "http://specs.openid.net/auth/2.0",
    "openid.mode": "id_res",
    "openid.op_endpoint": "https://steamcommunity.com/openid/login",
    "openid.claimed_id": "https://steamcommunity.com/openid/id/76561198000000001",
    "openid.identity": "https://steamcommunity.com/openid/id/76561198000000001",
    "openid.return_to": "https://cp.example.com/v1/steam/return?login=abc",
    "openid.sig": "deadbeef",
    "openid.signed": "signed,op_endpoint,claimed_id,identity,return_to",
    ...over,
  });
}

const RETURN_TO = "https://cp.example.com/v1/steam/return";

/** A Steam that confirms everything. */
const saysValid = async () => new Response("ns:http://specs.openid.net/auth/2.0\nis_valid:true\n");
/** A Steam that confirms nothing. */
const saysInvalid = async () =>
  new Response("ns:http://specs.openid.net/auth/2.0\nis_valid:false\n");

describe("steamIdFromClaimedId", () => {
  it("takes a real Steam claimed identifier", () => {
    expect(steamIdFromClaimedId("https://steamcommunity.com/openid/id/76561198000000001")).toBe(
      "76561198000000001",
    );
  });

  it("refuses an identifier from anywhere but Steam", () => {
    // The whole identity rests on this prefix. A look-alike host that happened to answer
    // an OpenID check would otherwise mint any identity it liked.
    for (const bad of [
      "https://steamcommunity.com.evil.test/openid/id/76561198000000001",
      "http://steamcommunity.com/openid/id/76561198000000001",
      "https://example.com/openid/id/76561198000000001",
    ]) {
      expect(steamIdFromClaimedId(bad), bad).toBeNull();
    }
  });

  it("refuses anything that isn't a SteamID64", () => {
    for (const bad of [
      "https://steamcommunity.com/openid/id/",
      "https://steamcommunity.com/openid/id/12345",
      "https://steamcommunity.com/openid/id/7656119800000000x",
      "https://steamcommunity.com/openid/id/765611980000000012",
      undefined,
      42,
    ]) {
      expect(steamIdFromClaimedId(bad as unknown), String(bad)).toBeNull();
    }
  });
});

describe("returnToMatches", () => {
  it("accepts our own return URL whatever the query carries", () => {
    expect(returnToMatches("https://cp.example.com/v1/steam/return?login=abc", RETURN_TO)).toBe(
      true,
    );
  });

  it("refuses an assertion minted for another site", () => {
    // This is the attack the check exists for: an assertion a user genuinely obtained at
    // some other Steam-login site verifies perfectly at Valve, because it is real. It is
    // just not about us.
    expect(returnToMatches("https://other.example.com/v1/steam/return", RETURN_TO)).toBe(false);
  });

  it("refuses a different path on our own host", () => {
    expect(returnToMatches("https://cp.example.com/v1/something/else", RETURN_TO)).toBe(false);
  });

  it("refuses junk", () => {
    expect(returnToMatches("not a url", RETURN_TO)).toBe(false);
    expect(returnToMatches(undefined, RETURN_TO)).toBe(false);
  });
});

describe("verifyAssertion", () => {
  it("returns the SteamID when Steam confirms it", async () => {
    const r = await verifyAssertion(assertion(), RETURN_TO, saysValid as typeof fetch);
    expect(isVerified(r) && r.steamId).toBe("76561198000000001");
  });

  it("asks Steam with mode swapped to check_authentication", async () => {
    // The verification is the entire security of this flow; if the echo back were sent
    // with the original mode, Steam would answer a different question.
    let sent: URLSearchParams | null = null;
    const spy = (async (_url: string, init: RequestInit) => {
      sent = new URLSearchParams(String(init.body));
      return new Response("is_valid:true");
    }) as unknown as typeof fetch;

    await verifyAssertion(assertion(), RETURN_TO, spy);
    expect(sent!.get("openid.mode")).toBe("check_authentication");
    expect(sent!.get("openid.sig")).toBe("deadbeef");
  });

  it("refuses when Steam says no", async () => {
    const r = await verifyAssertion(assertion(), RETURN_TO, saysInvalid as typeof fetch);
    expect(isVerified(r)).toBe(false);
  });

  it("refuses an assertion for another site even when Steam would confirm it", async () => {
    const r = await verifyAssertion(
      assertion({ "openid.return_to": "https://other.example.com/v1/steam/return" }),
      RETURN_TO,
      saysValid as typeof fetch,
    );
    expect(isVerified(r)).toBe(false);
  });

  it("refuses a non-Steam claimed id even when Steam would confirm it", async () => {
    const r = await verifyAssertion(
      assertion({ "openid.claimed_id": "https://example.com/openid/id/76561198000000001" }),
      RETURN_TO,
      saysValid as typeof fetch,
    );
    expect(isVerified(r)).toBe(false);
  });

  it("treats a user cancelling as a cancellation, not a failure", async () => {
    const r = await verifyAssertion(
      assertion({ "openid.mode": "cancel" }),
      RETURN_TO,
      saysValid as typeof fetch,
    );
    expect(isVerified(r)).toBe(false);
    expect((r as { error: string }).error).toBe("cancelled");
  });

  it("fails closed when Steam is unreachable", async () => {
    // No path may resolve a verification failure as success — least of all the one that
    // fires when the thing doing the verifying is down.
    const dead = (async () => {
      throw new Error("network");
    }) as unknown as typeof fetch;
    expect(isVerified(await verifyAssertion(assertion(), RETURN_TO, dead))).toBe(false);
  });

  it("fails closed on an error status from Steam", async () => {
    const boom = (async () => new Response("nope", { status: 503 })) as unknown as typeof fetch;
    expect(isVerified(await verifyAssertion(assertion(), RETURN_TO, boom))).toBe(false);
  });

  it("does not mistake is_valid:true inside another field for the verdict", async () => {
    // The response is line-oriented key:value. A value that merely contains the string
    // must not be read as the answer.
    const sneaky = (async () =>
      new Response("ns:http://x/\nnote:is_valid:true was not said\nis_valid:false\n")) as unknown as typeof fetch;
    expect(isVerified(await verifyAssertion(assertion(), RETURN_TO, sneaky))).toBe(false);
  });
});

describe("loginUrl", () => {
  it("asks Steam to pick the signed-in account and come back to us", () => {
    const url = new URL(loginUrl("https://cp.example.com/v1/steam/return?login=abc", "https://cp.example.com/"));
    expect(url.origin + url.pathname).toBe("https://steamcommunity.com/openid/login");
    expect(url.searchParams.get("openid.mode")).toBe("checkid_setup");
    expect(url.searchParams.get("openid.return_to")).toBe(
      "https://cp.example.com/v1/steam/return?login=abc",
    );
    expect(url.searchParams.get("openid.identity")).toBe(
      "http://specs.openid.net/auth/2.0/identifier_select",
    );
  });
});
