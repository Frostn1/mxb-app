/**
 * The chrome, and what the search box does with what was typed.
 *
 * Two things are being pinned. The first is that the tabs are the only way between the three
 * dashboards now that the cross-links are gone: a tab that loses the admin key is a tab that
 * lands on a 401, and no page would say so until somebody clicked it. The second is that the
 * search term is bounded before it reaches a query, the same way every other box on these
 * pages bounds it.
 */

import { describe, expect, it } from "vitest";
import { ranges, shell, type Ctx } from "../src/adminui";
import { parseQuery } from "../src/adminsearch";

const c: Ctx = { key: "s3cret", back: "/admin/usage?key=s3cret" };
const at = (query: string) => new URL(`https://example.com/admin/search${query}`);

describe("what was typed", () => {
  it("is what comes back", () => {
    expect(parseQuery(at("?q=frost"))).toBe("frost");
  });

  it("is empty when there is no box to read", () => {
    expect(parseQuery(at(""))).toBe("");
  });

  it("loses the whitespace around it", () => {
    expect(parseQuery(at("?q=%20%20frost%20%20"))).toBe("frost");
  });

  it("stops at the length every other search box stops at", () => {
    expect(parseQuery(at(`?q=${"x".repeat(400)}`))).toHaveLength(96);
  });
});

describe("the tabs", () => {
  it("carry the key, or every one of them lands on a 401", () => {
    const page = shell({ title: "Usage", section: "usage", body: "", c });
    for (const path of ["/admin", "/admin/diagnostics", "/admin/paints"]) {
      expect(page).toContain(`href="${path}?key=s3cret"`);
    }
  });

  it("point the usage tab at the URL worth bookmarking, not at its alias", () => {
    const page = shell({ title: "Usage", section: "usage", body: "", c });
    expect(page).toContain(`<a class="on" href="/admin?key=s3cret">Usage</a>`);
    expect(page).not.toContain("/admin/usage");
  });

  it("light the section being looked at, and only that one", () => {
    const page = shell({ title: "Paints", section: "paints", body: "", c });
    expect(page).toContain(`<a class="on" href="/admin/paints?key=s3cret">Paints</a>`);
    expect(page).toContain(`<a class="" href="/admin?key=s3cret">Usage</a>`);
    expect(page).toContain(`<a class="" href="/admin/diagnostics?key=s3cret">Diagnostics</a>`);
  });

  it("light none of them on the page that belongs to no section", () => {
    const page = shell({ title: "Search", section: null, body: "", c });
    expect(page).not.toContain(`class="on"`);
  });

  it("are on every page, which is what makes one browser tab enough", () => {
    for (const section of ["usage", "diagnostics", "paints"] as const) {
      const page = shell({ title: "x", section, body: "", c });
      expect(page).toContain(">Usage</a>");
      expect(page).toContain(">Diagnostics</a>");
      expect(page).toContain(">Paints</a>");
    }
  });
});

describe("the second row", () => {
  const tabs = [
    { id: "riders", text: "Riders", path: "/admin/paints" },
    { id: "paints", text: "Paints", path: "/admin/paints/files" },
  ];

  it("marks the view being looked at", () => {
    const page = shell({ title: "Paints", section: "paints", tabs, current: "paints", body: "", c });
    expect(page).toContain(`<a class="on" href="/admin/paints/files?key=s3cret">Paints</a>`);
  });

  it("is left out entirely when a section has no views and no window", () => {
    const page = shell({ title: "Usage", section: "usage", body: "", c });
    expect(page).not.toContain(`<div class="subbar">`);
  });

  it("is drawn for a window alone, which is all the usage tab has", () => {
    const page = shell({ title: "Usage", section: "usage", aside: "<a>30d</a>", body: "", c });
    expect(page).toContain(`<div class="subbar">`);
  });
});

describe("the search box", () => {
  it("holds the query the page is showing, so it can be edited rather than retyped", () => {
    expect(shell({ title: "Search", section: null, body: "", q: "frost", c })).toContain(
      `value="frost"`,
    );
  });

  it("takes the key with it, since a GET form only sends its own fields", () => {
    expect(shell({ title: "Search", section: null, body: "", c })).toContain(
      `<input type="hidden" name="key" value="s3cret">`,
    );
  });

  it("has no key field to send when there is no key", () => {
    const page = shell({ title: "Search", section: null, body: "", c: { key: "", back: "" } });
    expect(page).toContain(`action="/admin/search"`);
    expect(page).not.toContain(`name="key"`);
  });
});

describe("a title", () => {
  it("is escaped — a rider name and a file name both reach it", () => {
    const page = shell({ title: `<script>x</script>`, section: null, body: "", c });
    expect(page).not.toContain("<script>x</script>");
    expect(page).toContain("&lt;script&gt;x&lt;/script&gt;");
  });
});

describe("the day-window switcher", () => {
  it("marks the window being looked at", () => {
    // `&amp;` rather than `&`: these go into an attribute, and `esc` is what puts them there.
    const html = ranges(30, [7, 30, 90], "/admin/usage", {}, c);
    expect(html).toContain(`<a class="on" href="/admin/usage?days=30&amp;key=s3cret">30d</a>`);
    expect(html).toContain(`<a class="" href="/admin/usage?days=7&amp;key=s3cret">7d</a>`);
  });

  it("drops the page number — page 4 of a week is not page 4 of a year", () => {
    expect(ranges(7, [7], "/admin/diagnostics", { page: 4 }, c)).not.toContain("page=");
  });
});
