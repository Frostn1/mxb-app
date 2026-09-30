import { describe, expect, it } from "vitest";
import { applyRace, updateRating, type Glicko } from "../src/glicko2";

// Glickman's own worked example ("Example of the Glicko-2 system"), §1 "Step-by-step example":
// a player rated 1500/200/0.06 plays three opponents rated 1400/30, 1550/100 and 1700/300,
// winning against the first and losing the other two. The paper carries these through to
// rating 1464.06, RD 151.52, volatility 0.05999.
describe("updateRating", () => {
  it("matches Glickman's worked example to the precision the paper publishes", () => {
    const player: Glicko = { rating: 1500, rd: 200, volatility: 0.06 };
    const result = updateRating(
      player,
      [
        { rating: 1400, rd: 30, score: 1 },
        { rating: 1550, rd: 100, score: 0 },
        { rating: 1700, rd: 300, score: 0 },
      ],
      0.5,
    );
    expect(result.rating).toBeCloseTo(1464.06, 1);
    expect(result.rd).toBeCloseTo(151.52, 1);
    expect(result.volatility).toBeCloseTo(0.05999, 4);
  });

  it("grows RD toward uncertainty and leaves rating/volatility alone with no games", () => {
    const player: Glicko = { rating: 1500, rd: 200, volatility: 0.06 };
    const result = updateRating(player, []);
    expect(result.rating).toBe(1500);
    expect(result.rd).toBeGreaterThan(200);
    expect(result.volatility).toBe(0.06);
  });

  it("is symmetric: a win and a loss against an equally-rated opponent move rating by the same amount in opposite directions", () => {
    const player: Glicko = { rating: 1500, rd: 100, volatility: 0.06 };
    const opponent = { rating: 1500, rd: 100, volatility: 0.06 };
    const win = updateRating(player, [{ rating: opponent.rating, rd: opponent.rd, score: 1 }]);
    const loss = updateRating(player, [{ rating: opponent.rating, rd: opponent.rd, score: 0 }]);
    expect(win.rating - 1500).toBeCloseTo(-(loss.rating - 1500), 6);
  });
});

describe("applyRace", () => {
  it("rates 1st over 2nd over 3rd simultaneously, off the same pre-race ratings", () => {
    const before: Glicko = { rating: 1500, rd: 200, volatility: 0.06 };
    const after = applyRace([
      { guid: "A", before },
      { guid: "B", before },
      { guid: "C", before },
    ]);
    const a = after.get("A")!;
    const b = after.get("B")!;
    const c = after.get("C")!;
    // Winner gains the most, last place loses the most, all off an identical starting point.
    expect(a.rating).toBeGreaterThan(b.rating);
    expect(b.rating).toBeGreaterThan(c.rating);
    expect(a.rating - 1500).toBeCloseTo(-(c.rating - 1500), 6);
  });

  it("matches updateRating's own two-game result for a three-rider race", () => {
    const winner: Glicko = { rating: 1600, rd: 80, volatility: 0.06 };
    const loser1: Glicko = { rating: 1500, rd: 120, volatility: 0.06 };
    const loser2: Glicko = { rating: 1400, rd: 150, volatility: 0.06 };
    const after = applyRace([
      { guid: "W", before: winner },
      { guid: "L1", before: loser1 },
      { guid: "L2", before: loser2 },
    ]);
    const direct = updateRating(winner, [
      { rating: loser1.rating, rd: loser1.rd, score: 1 },
      { rating: loser2.rating, rd: loser2.rd, score: 1 },
    ]);
    expect(after.get("W")!.rating).toBeCloseTo(direct.rating, 6);
    expect(after.get("W")!.rd).toBeCloseTo(direct.rd, 6);
  });
});
