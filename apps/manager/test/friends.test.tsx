import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nContext, type I18nContextValue } from "@frost/shared/i18n/context";
import type { Friend, FriendsState } from "../src/api/friends";
import FriendsPanel from "../src/Components/Servers/FriendsPanel";
import {
  foldName,
  friendsByAddress,
  looksLikeCode,
  serverForFriend,
  sortFriends,
} from "../src/lib/friends";

const presence = (serverName: string, address: string | null = null) => ({
  serverName,
  address,
  track: null,
  riders: 4,
  updatedAt: 1,
});
const friend = (riderName: string, p: Friend["presence"]): Friend => ({
  accountId: `acc_${riderName.toLowerCase()}`,
  riderName,
  presence: p,
});

const servers = [
  { name: "Fake Night League", address: "203.0.113.7:54210", players: 2 },
  { name: "Fake  Night League", address: "198.51.100.2:54210", players: 9 },
  { name: "Other Fake Server", address: "192.0.2.5:54210", players: 1 },
];

test("a server name is folded the way the session block and control plane fold it", () => {
  expect(foldName("  Fake   Night\tLeague ")).toBe("fake night league");
});

test("an address match beats a name match", () => {
  const f = friend("Ann", presence("Fake Night League", "203.0.113.7:54210"));
  expect(serverForFriend(f, servers)?.address).toBe("203.0.113.7:54210");
});

test("without an address the busier of two same-named servers wins", () => {
  const f = friend("Ann", presence("fake night league"));
  expect(serverForFriend(f, servers)?.address).toBe("198.51.100.2:54210");
});

test("a friend on an unlisted server, or offline, matches nothing", () => {
  expect(serverForFriend(friend("Ann", presence("Nowhere Fake")), servers)).toBeNull();
  expect(serverForFriend(friend("Bob", null), servers)).toBeNull();
});

test("friends are grouped under the address the browser lists", () => {
  const by = friendsByAddress(
    [
      friend("Ann", presence("Other Fake Server")),
      friend("Bob", presence("Other Fake Server", "192.0.2.5:54210")),
      friend("Cy", null),
    ],
    servers,
  );
  expect(Object.keys(by)).toEqual(["192.0.2.5:54210"]);
  expect(by["192.0.2.5:54210"]!.map((f) => f.riderName)).toEqual(["Ann", "Bob"]);
});

test("online friends sort ahead of offline ones", () => {
  const sorted = sortFriends([friend("Zed", presence("S")), friend("Amy", null), friend("Ben", presence("S"))]);
  expect(sorted.map((f) => f.riderName)).toEqual(["Ben", "Zed", "Amy"]);
});

test("a friend code reads as a code, a short name as a name", () => {
  expect(looksLikeCode("ABCD-EFG2")).toBe(true);
  expect(looksLikeCode("abcd efgh")).toBe(true);
  expect(looksLikeCode("ABCDEF23")).toBe(true);
  expect(looksLikeCode("Motorist")).toBe(false);
  expect(looksLikeCode("Ann")).toBe(false);
});

const ctx: I18nContextValue = { locale: "en", resolved: "en", setLocale: () => {}, t: (k: string) => k };

const state: FriendsState = {
  friendCode: "ABCD-EFG2",
  hidePresence: false,
  friends: [friend("Ann", presence("Fake Night League", "203.0.113.7:54210")), friend("Bob", null)],
  incoming: [{ accountId: "acc_cy", riderName: "Cy" }],
  outgoing: [],
};

test("the panel lists friends, requests and the code, and joins through the tab's own action", () => {
  const html = renderToStaticMarkup(
    <I18nContext.Provider value={ctx}>
      <FriendsPanel
        state={state}
        servers={servers}
        actionFor={() => "install"}
        onJoin={() => {}}
        onChanged={() => {}}
      />
    </I18nContext.Provider>,
  );
  for (const text of ["Ann", "Bob", "Cy", "ABCD-EFG2", "friends.accept", "friends.installJoin", "friends.offline"]) {
    expect(html).toContain(text);
  }
  // Bob is offline, so only Ann has a join button.
  expect(html.match(/friends\.installJoin/g)?.length).toBe(1);
});
