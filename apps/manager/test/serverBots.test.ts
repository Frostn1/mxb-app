import { expect, test } from "bun:test";
import { splitBots, type MasterServer } from "@frost/shared/api/mods";

const row = (location: string) => ({ location }) as MasterServer;

test("an MXB native server's bot tag moves out of its location", () => {
  expect(splitBots(row("+15 bots"))).toMatchObject({ location: "", bots: 15 });
  expect(splitBots(row("EU +1 bot"))).toMatchObject({ location: "EU", bots: 1 });
});

test("any other location is left alone with no bots", () => {
  for (const location of ["USA", "", "EU West", "bots +15", "2+15 bots"]) {
    expect(splitBots(row(location))).toMatchObject({ location, bots: 0 });
  }
});
