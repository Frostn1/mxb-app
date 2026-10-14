import type { HealthReport } from "@frost/shared/types";

/**
 * The tyre notices, as the bars show them: `empty` folders crash the bike list (loud),
 * `overrides` replace a stock tyre (quiet). Each is the joined names, or null for no bar.
 */
export function tyreNotices(report: HealthReport): {
  empty: null | { names: string; count: number };
  overrides: null | { names: string; count: number };
} {
  const bar = (list: string[]) =>
    list.length === 0 ? null : { names: list.join(", "), count: list.length };
  return { empty: bar(report.tyres.empty), overrides: bar(report.tyres.overrides) };
}
