import { useSyncExternalStore } from "react";
import { readTipsState, subscribeTips, type TipsState } from "../../lib/tips";

let snapshot: TipsState = readTipsState();
const subscribe = (fn: () => void) =>
  subscribeTips(() => {
    snapshot = readTipsState();
    fn();
  });
const getSnapshot = () => snapshot;

/** The remembered tips record, shared live by the home card and Settings → Tips. */
export function useTipsState(): TipsState {
  return useSyncExternalStore(subscribe, getSnapshot);
}
