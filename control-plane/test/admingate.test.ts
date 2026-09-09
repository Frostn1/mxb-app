import { describe, expect, it } from "vitest";

import { adminAllowed, windowDays } from "../src/auth";

/**
 * The admin key gate, and the window a dashboard asks for.
 *
 * Both used to live beside the anonymous usage counters and were covered by that module's
 * tests. The counters are gone; these two are not — every admin page is behind them — so the
 * coverage moves here rather than leaving with the feature.
 */
describe("who may reach an admin page", () => {
  const url = (query = "") => new URL(`https://cp.test/admin/paints${query}`);
  const plain = new Request("https://cp.test/admin/paints");

  it("has no admin surface at all on a deployment with no key", () => {
    expect(adminAllowed(plain, url(), {} as Env)).toBe("unset");
  });

  it("turns away a request with no key", () => {
    expect(adminAllowed(plain, url(), { ADMIN_KEY: "s3cret" } as Env)).toBe("denied");
  });

  it("turns away the wrong key", () => {
    expect(adminAllowed(plain, url("?key=guess"), { ADMIN_KEY: "s3cret" } as Env)).toBe("denied");
  });

  it("takes the key from the query, because a browser cannot send a header", () => {
    expect(adminAllowed(plain, url("?key=s3cret"), { ADMIN_KEY: "s3cret" } as Env)).toBe("ok");
  });

  it("takes a bearer token too, for anything scripting it", () => {
    const req = new Request("https://cp.test/admin/paints", {
      headers: { Authorization: "Bearer s3cret" },
    });
    expect(adminAllowed(req, url(), { ADMIN_KEY: "s3cret" } as Env)).toBe("ok");
  });
});

describe("the window a dashboard asks for", () => {
  it("defaults to a month", () => {
    expect(windowDays(new URL("https://cp.test/admin/paints"))).toBe(30);
  });

  it("clamps something absurd rather than scanning the whole history", () => {
    expect(windowDays(new URL("https://cp.test/admin/paints?days=99999"))).toBe(365);
    expect(windowDays(new URL("https://cp.test/admin/paints?days=-4"))).toBe(1);
    expect(windowDays(new URL("https://cp.test/admin/paints?days=nonsense"))).toBe(30);
  });
});
