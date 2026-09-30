/**
 * Glicko-2, the generic algorithm (Glickman, "Example of the Glicko-2 system").
 *
 * Kept as pure math with no D1, no GUIDs, no classes — `rating.ts` is the only caller and
 * owns every domain decision (who counts, what a win is, which class). This file only knows
 * "a rating played these games against these opponents, who won".
 */

/** Public-scale rating, RD and volatility — the numbers stored in `rider_ratings`. */
export interface Glicko {
  rating: number;
  rd: number;
  volatility: number;
}

/** One game from the rated rider's point of view: 1 = win, 0.5 = draw, 0 = loss. */
export interface Opponent {
  rating: number;
  rd: number;
  score: 0 | 0.5 | 1;
}

/** Glicko-2's own scale is 173.7178 rating points per unit — Glickman's constant, not tuned. */
const SCALE = 173.7178;
const DEFAULT_RATING = 1500;

/** Default new-rider rating: high RD (unproven), mid volatility. Never itself "rated". */
export const DEFAULT_GLICKO: Glicko = { rating: 1500, rd: 350, volatility: 0.06 };

/** System constant restraining volatility swings. 0.3–1.2 is Glickman's recommended band. */
const DEFAULT_TAU = 0.5;
const CONVERGENCE = 0.000001;

function g(phi: number): number {
  return 1 / Math.sqrt(1 + (3 * phi * phi) / (Math.PI * Math.PI));
}

function E(mu: number, muJ: number, phiJ: number): number {
  return 1 / (1 + Math.exp(-g(phiJ) * (mu - muJ)));
}

/**
 * One rating-period update for a single rider against 0+ opponents in that period.
 *
 * A race is one period per the design ("applied per race in order") — every pairwise
 * comparison within the race is one "game" here, and the returned rating is what the next
 * race in ingestion order starts from. Deterministic: same inputs, same output, no clock and
 * no randomness anywhere in the walk.
 */
export function updateRating(player: Glicko, opponents: Opponent[], tau = DEFAULT_TAU): Glicko {
  const mu = (player.rating - DEFAULT_RATING) / SCALE;
  const phi = player.rd / SCALE;
  const sigma = player.volatility;

  if (opponents.length === 0) {
    // Step 6 only: RD grows toward uncertainty, rating and volatility hold.
    const phiStar = Math.sqrt(phi * phi + sigma * sigma);
    return { rating: player.rating, rd: phiStar * SCALE, volatility: sigma };
  }

  const games = opponents.map((o) => ({
    muJ: (o.rating - DEFAULT_RATING) / SCALE,
    phiJ: o.rd / SCALE,
    score: o.score,
  }));

  // Step 3: estimated variance of the rating over the opponents faced.
  let vInv = 0;
  for (const gm of games) {
    const gj = g(gm.phiJ);
    const ej = E(mu, gm.muJ, gm.phiJ);
    vInv += gj * gj * ej * (1 - ej);
  }
  const v = 1 / vInv;

  // Step 4: delta, the estimated improvement in rating.
  let sum = 0;
  for (const gm of games) {
    sum += g(gm.phiJ) * (gm.score - E(mu, gm.muJ, gm.phiJ));
  }
  const delta = v * sum;

  // Step 5: new volatility via the Illinois algorithm on f(x) (Glickman's worked example).
  const a = Math.log(sigma * sigma);
  const f = (x: number): number => {
    const ex = Math.exp(x);
    const num = ex * (delta * delta - phi * phi - v - ex);
    const den = 2 * (phi * phi + v + ex) * (phi * phi + v + ex);
    return num / den - (x - a) / (tau * tau);
  };

  let A = a;
  let B: number;
  if (delta * delta > phi * phi + v) {
    B = Math.log(delta * delta - phi * phi - v);
  } else {
    let k = 1;
    while (f(a - k * tau) < 0) k++;
    B = a - k * tau;
  }

  let fA = f(A);
  let fB = f(B);
  while (Math.abs(B - A) > CONVERGENCE) {
    const C = A + ((A - B) * fA) / (fB - fA);
    const fC = f(C);
    if (fC * fB < 0) {
      A = B;
      fA = fB;
    } else {
      fA = fA / 2;
    }
    B = C;
    fB = fC;
  }
  const newSigma = Math.exp(A / 2);

  // Step 6: pre-period value with the new volatility folded in.
  const phiStar = Math.sqrt(phi * phi + newSigma * newSigma);

  // Step 7: new RD and rating, on the Glicko-2 scale then converted back.
  const newPhi = 1 / Math.sqrt(1 / (phiStar * phiStar) + 1 / v);
  const newMu = mu + newPhi * newPhi * sum;

  return {
    rating: SCALE * newMu + DEFAULT_RATING,
    rd: SCALE * newPhi,
    volatility: newSigma,
  };
}

/**
 * A race's finish order turned into one Glicko-2 update per rated rider, applied
 * simultaneously — every rider's opponents are everyone else in `order`, and all of them are
 * computed from the *same* pre-race ratings (Glickman's "simultaneous", not sequential, so
 * the result does not depend on iteration order within the race).
 *
 * `order` is finish order best-to-worst, already filtered to the riders this race rates
 * (`rating.ts` owns the human/bot/laps/count filtering — this function trusts its input).
 */
export function applyRace(
  order: { guid: string; before: Glicko }[],
  tau = DEFAULT_TAU,
): Map<string, Glicko> {
  const after = new Map<string, Glicko>();
  for (let i = 0; i < order.length; i++) {
    const rider = order[i];
    // Everyone else in the race, scored by whether `rider` finished ahead of them — `order`
    // is best-to-worst, so a lower index beat a higher one.
    const opponents: Opponent[] = [];
    for (let j = 0; j < order.length; j++) {
      if (j === i) continue;
      opponents.push({
        rating: order[j].before.rating,
        rd: order[j].before.rd,
        score: i < j ? 1 : 0,
      });
    }
    after.set(rider.guid, updateRating(rider.before, opponents, tau));
  }
  return after;
}
