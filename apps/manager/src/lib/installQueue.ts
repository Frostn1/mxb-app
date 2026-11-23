export const TIMED_OUT = Symbol("TIMED_OUT");

export function withTimeout<T>(
  promise: Promise<T>,
  ms: number,
): Promise<T | typeof TIMED_OUT> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => resolve(TIMED_OUT), ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

export type ReviewOutcome =
  | { kind: "installed"; installed: number; failed: number; firstError?: string }
  | { kind: "discarded" };

export type ReviewOutcomeAction =
  | { stage: "done" }
  | { stage: "error"; message: string }
  | { remove: true };

export function reviewOutcomeStage(
  outcome: ReviewOutcome,
  fallbackError = "Install failed",
): ReviewOutcomeAction {
  if (outcome.kind === "discarded") return { remove: true };
  if (outcome.installed > 0) return { stage: "done" };
  return { stage: "error", message: outcome.firstError ?? fallbackError };
}
