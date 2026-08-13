import { describe, expect, it } from "vitest";
import { bearer } from "../src/auth";
import { bootstrapScript } from "../src/bootstrap";
import {
  isBikeId,
  isBootstrapStage,
  isGuid,
  isPaintFileName,
  isPaintSize,
  isPublicAgentUrl,
  isPublicGameAddress,
  isRegion,
  isRelDest,
  isServerKey,
  isRiderName,
  isServerName,
  isSha256,
  isSlot,
  MAX_PAINT_BYTES,
} from "../src/validate";

describe("paint filenames", () => {
  it("accepts an ordinary paint", () => {
    expect(isPaintFileName("Alpinestars.pnt")).toBe(true);
    expect(isPaintFileName("  my paint 2026.PNT  ")).toBe(true);
  });

  it("rejects anything that could escape the paints folder", () => {
    // This value becomes a path on another player's disk, so traversal is the one that
    // actually matters — everything else here is defence in depth.
    for (const bad of [
      "../../mxbikes.ini",
      "..\\..\\core.ini",
      "sub/dir.pnt",
      "sub\\dir.pnt",
      "..pnt.pnt/../x.pnt",
    ]) {
      expect(isPaintFileName(bad), bad).toBe(false);
    }
  });

  it("rejects names Windows cannot hold, and control characters", () => {
    for (const bad of ['a:b.pnt', 'a*b.pnt', 'a?b.pnt', 'a"b.pnt', "a<b.pnt", "a>b.pnt", "a|b.pnt", "a\u0000b.pnt"]) {
      expect(isPaintFileName(bad), JSON.stringify(bad)).toBe(false);
    }
  });

  it("requires the extension the game actually loads", () => {
    expect(isPaintFileName("livery.png")).toBe(false);
    expect(isPaintFileName("livery")).toBe(false);
    expect(isPaintFileName("")).toBe(false);
  });
});

describe("destination paths", () => {
  it("accepts the layout the game actually uses", () => {
    expect(isRelDest("bikes/2026 KTM 450/paints/Frost.pnt")).toBe(true);
    expect(isRelDest("rider/helmets/Airoh/paints/Frost.pnt")).toBe(true);
  });

  it("refuses to escape the mods folder", () => {
    // One player uploads this and another player's app joins it onto a real directory —
    // this is the value that would actually write outside the mods folder.
    for (const bad of [
      "../mxbikes.ini",
      "bikes/../../../mxbikes.ini",
      "/etc/passwd",
      "C:/Windows/system32/x.pnt",
      "c:\\windows\\x.pnt",
      "bikes\\ktm\\paints\\x.pnt",
      "bikes//paints/x.pnt",
      "./x.pnt",
    ]) {
      expect(isRelDest(bad), bad).toBe(false);
    }
  });

  it("still requires the last segment to be a paint", () => {
    expect(isRelDest("bikes/ktm/paints/notapaint.txt")).toBe(false);
    expect(isRelDest("bikes/ktm/paints")).toBe(false);
  });

  it("rejects control characters and absurd lengths", () => {
    expect(isRelDest("bikes/\u0000/x.pnt")).toBe(false);
    expect(isRelDest("a/".repeat(200) + "x.pnt")).toBe(false);
    expect(isRelDest(42)).toBe(false);
  });
});

describe("rider names", () => {
  it("accepts names the roster could report back", () => {
    expect(isRiderName("Frost")).toBe(true);
    expect(isRiderName("Jean-Luc #47")).toBe(true);
  });

  it("rejects control characters, since the name must survive the round trip", () => {
    expect(isRiderName("Frost\u0000")).toBe(false);
    expect(isRiderName("Frost\u001b[31m")).toBe(false);
  });

  it("rejects lengths that are not a real rider", () => {
    expect(isRiderName("a")).toBe(false);
    expect(isRiderName("x".repeat(65))).toBe(false);
    expect(isRiderName(42)).toBe(false);
  });
});

describe("content addressing", () => {
  it("accepts a lowercase sha-256 and nothing else", () => {
    expect(isSha256("a".repeat(64))).toBe(true);
    expect(isSha256("A".repeat(64))).toBe(false);
    expect(isSha256("a".repeat(63))).toBe(false);
    expect(isSha256("../etc/passwd")).toBe(false);
  });

  it("bounds paint sizes", () => {
    expect(isPaintSize(1)).toBe(true);
    expect(isPaintSize(MAX_PAINT_BYTES)).toBe(true);
    expect(isPaintSize(MAX_PAINT_BYTES + 1)).toBe(false);
    expect(isPaintSize(0)).toBe(false);
    expect(isPaintSize(-1)).toBe(false);
    expect(isPaintSize(1.5)).toBe(false);
  });
});

describe("player GUIDs", () => {
  it("accepts plausible opaque identifiers", () => {
    expect(isGuid("ab12cd34ef56")).toBe(true);
    expect(isGuid("A1B2-C3D4-E5F6")).toBe(true);
  });

  it("rejects anything with whitespace", () => {
    // The server log delimits the GUID by whitespace, so one containing any could never
    // have come from there.
    expect(isGuid("ab12 cd34")).toBe(false);
    expect(isGuid(" ")).toBe(false);
  });

  it("rejects lengths and characters that are not an identifier", () => {
    expect(isGuid("ab")).toBe(false);
    expect(isGuid("x".repeat(101))).toBe(false);
    expect(isGuid("../etc/passwd")).toBe(false);
    expect(isGuid(null)).toBe(false);
  });
});

describe("slots", () => {
  it("accepts the profile.ini section names the game uses", () => {
    expect(isSlot("paint")).toBe(true);
    expect(isSlot("goggles_paint")).toBe(true);
    expect(isSlot("protection_paint")).toBe(true);
    // Not a paint slot — models and non-paint settings must not carry a blob.
    expect(isSlot("tyres")).toBe(false);
    expect(isSlot("wheels")).toBe(false);
    expect(isSlot(null)).toBe(false);
  });
});

describe("server registration", () => {
  it("accepts a routable game address", () => {
    expect(isPublicGameAddress("203.0.113.10:54210")).toBe(true);
    expect(isPublicGameAddress("mx.example.com:54210")).toBe(true);
    expect(isPublicGameAddress(" 18.185.94.143:54210 ")).toBe(true);
  });

  it("rejects an address nobody outside could connect to", () => {
    // A home server published under its LAN address is a row in everyone's join picker
    // that can never work — and the port is what we would otherwise go and probe.
    for (const bad of [
      "127.0.0.1:54210",
      "localhost:54210",
      "10.0.0.5:54210",
      "192.168.1.20:54210",
      "172.16.4.4:54210",
      "169.254.169.254:80",
      "100.64.0.1:54210",
      "0.0.0.0:54210",
      "[::1]:54210",
    ]) {
      expect(isPublicGameAddress(bad), bad).toBe(false);
    }
  });

  it("rejects addresses that are malformed rather than merely private", () => {
    for (const bad of [
      "203.0.113.10", // no port
      "203.0.113.10:0",
      "203.0.113.10:70000",
      "-flag:54210",
      "203.0.113.10:54210 -log",
      "999.1.1.1:54210",
      "",
    ]) {
      expect(isPublicGameAddress(bad), bad).toBe(false);
    }
  });

  it("accepts an agent URL we are willing to call", () => {
    expect(isPublicAgentUrl("http://203.0.113.10:8787")).toBe(true);
    expect(isPublicAgentUrl("https://mx.example.com")).toBe(true);
    expect(isPublicAgentUrl("http://203.0.113.10:8787/")).toBe(true);
  });

  it("refuses agent URLs that would turn us into a probe", () => {
    // This value is fetched server-side, so each of these is a request-forgery attempt.
    for (const bad of [
      "http://127.0.0.1:8787",
      "http://localhost:8787",
      "http://169.254.169.254/latest/meta-data/",
      "http://10.1.2.3:8787",
      "http://[::1]:8787",
      "file:///etc/passwd",
      "ftp://203.0.113.10",
      "http://user:pass@203.0.113.10:8787",
      "http://203.0.113.10:8787/admin",
      "http://203.0.113.10:8787?x=1",
      "not a url",
      "",
    ]) {
      expect(isPublicAgentUrl(bad), bad).toBe(false);
    }
  });

  it("holds server names to something that fits a row", () => {
    expect(isServerName("Frost Test EU")).toBe(true);
    expect(isServerName("A")).toBe(false);
    expect(isServerName("x".repeat(49))).toBe(false);
    expect(isServerName("bad\nname")).toBe(false);
    expect(isServerName(null)).toBe(false);
  });

  it("takes regions from a closed set", () => {
    expect(isRegion("eu-central-1")).toBe(true);
    expect(isRegion("mars-north-1")).toBe(false);
    expect(isRegion("")).toBe(false);
  });
});

describe("bearer parsing", () => {
  it("takes the token and tolerates case and spacing", () => {
    expect(bearer("Bearer abc123")).toBe("abc123");
    expect(bearer("bearer   abc123  ")).toBe("abc123");
  });

  it("rejects other header shapes", () => {
    expect(bearer("Basic abc123")).toBe(null);
    expect(bearer("Bearer")).toBe(null);
    expect(bearer("Bearer   ")).toBe(null);
    expect(bearer(null)).toBe(null);
  });
});

describe("bike ids", () => {
  it("takes the names a profile.ini actually uses", () => {
    // Real keys out of the `[paint]` section: vendor names, years, spaces, dots.
    for (const ok of ["YZ450F", "2026 KTM 450 SX-F", "kx250_v1.2", "BSB23_Ducati_V4R"]) {
      expect(isBikeId(ok), ok).toBe(true);
    }
  });

  it("refuses anything that isn't a plain key", () => {
    // This became half a primary key and is echoed into every roster; before per-bike
    // loadouts it was stored with no validation at all.
    for (const bad of [
      "",
      "   ",
      "a/b",
      "a\\b",
      "bike\u0000",
      "bike\n450",
      "x".repeat(129),
      42,
      null,
      undefined,
      {},
    ]) {
      expect(isBikeId(bad), JSON.stringify(bad)).toBe(false);
    }
  });
});

describe("bootstrap stages", () => {
  it("takes the labels the bootstrap actually sends", () => {
    for (const ok of [
      "starting up",
      "downloading the game",
      "extracting the game",
      "installing the agent",
      "waiting for the agent",
      "ready",
      "failed",
    ]) {
      expect(isBootstrapStage(ok), ok).toBe(true);
    }
  });

  it("refuses anything that isn't a short plain label", () => {
    // Written by a script on a machine we launched and read straight back into the app's UI,
    // so it is checked rather than trusted for coming from our own instance.
    for (const bad of ["", "   ", "a".repeat(65), "stage\nnext", "<b>x</b>", "drop;table", 7, null]) {
      expect(isBootstrapStage(bad), JSON.stringify(bad)).toBe(false);
    }
  });
});

describe("bootstrap user data", () => {
  it("is plain ASCII, so a Windows host cannot misread it", () => {
    // EC2 hands the decoded script to PowerShell 5.1, which reads a file with no BOM using
    // the system codepage rather than UTF-8. Comments are harmless; a mangled command is not.
    const script = bootstrapScript({
      agentToken: "t", agentUrl: "https://cp/v1/agent.exe", gameUrl: "https://g/i.exe",
      serverName: "Test", gamePort: 54210, agentPort: 8787,
      serverId: "abc", controlPlaneUrl: "https://cp",
    });
    const offenders = [...new Set([...script].filter((c) => c.charCodeAt(0) > 127))];
    expect(offenders, `non-ASCII in the script: ${JSON.stringify(offenders)}`).toEqual([]);
  });

  it("survives a server name outside Latin-1", () => {
    // The name is the operator's, and it is interpolated into the script. `btoa` threw on
    // anything above U+00FF, which failed the launch with nothing to go on.
    expect(() =>
      bootstrapScript({
        agentToken: "t", agentUrl: "https://cp/v1/agent.exe", gameUrl: "https://g/i.exe",
        serverName: "Sean 🏁 サーバー", gamePort: 54210, agentPort: 8787,
        serverId: "abc", controlPlaneUrl: "https://cp",
      }),
    ).not.toThrow();
  });
});

describe("server keys", () => {
  it("takes both shapes the app computes", () => {
    // A registry id for a server we run, a normalized host:port for one we do not.
    for (const ok of ["eu-frankfurt-1", "203.0.113.10:54210", "8e68ebe5-ec6e-42dd"]) {
      expect(isServerKey(ok), ok).toBe(true);
    }
  });

  it("refuses what it should not store", () => {
    const withNewline = "srv" + String.fromCharCode(10) + "other";
    for (const bad of ["", "   ", "a".repeat(129), withNewline, 5, null]) {
      expect(isServerKey(bad), JSON.stringify(bad)).toBe(false);
    }
  });
});
