/**
 * Which manufacturer a bike is, read off its name.
 *
 * The app ships no manufacturer artwork and shouldn't: those marks are trademarks, and a
 * mod folder has nothing in it that would license one. What a card can honestly carry is the
 * name, set in the app's own condensed face — enough to tell a KTM from a Kawasaki at a
 * glance across a grid, which is the whole reason the plain block was a waste of the tile.
 *
 * Matching is on whole words only. A bike is named `MX1OEM_2023_KTM_450_SX-F` or
 * `2024 Yamaha YZ250F by …`, so the brand is always a word of its own; substring matching
 * would find "tm" inside half the catalog.
 */

/** Spellings that are the same maker. The value is how the card writes it. */
const ALIASES: Record<string, string> = {
  ktm: "KTM",
  husqvarna: "Husqvarna",
  husky: "Husqvarna",
  honda: "Honda",
  hrc: "Honda",
  yamaha: "Yamaha",
  yami: "Yamaha",
  kawasaki: "Kawasaki",
  kawa: "Kawasaki",
  suzuki: "Suzuki",
  suzi: "Suzuki",
  gasgas: "GasGas",
  beta: "Beta",
  sherco: "Sherco",
  fantic: "Fantic",
  triumph: "Triumph",
  stark: "Stark",
  rieju: "Rieju",
  kove: "Kove",
  alta: "Alta",
};

/** Two words that only name a maker together — checked before the single-word pass. */
const PAIRS: Record<string, string> = { "gas gas": "GasGas" };

export function brandOf(...names: (string | null | undefined)[]): string | null {
  for (const name of names) {
    if (!name) continue;
    // Underscores, dots and dashes are all word breaks in a bike folder's name.
    const words = name.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);
    for (let i = 0; i < words.length - 1; i++) {
      const pair = PAIRS[`${words[i]} ${words[i + 1]}`];
      if (pair) return pair;
    }
    for (const word of words) {
      const hit = ALIASES[word];
      if (hit) return hit;
    }
  }
  return null;
}
