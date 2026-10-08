import { createContext, useContext } from "react";
import type { DashboardView } from "../Shell/nav";

/**
 * How a tip's action moves the app: open a page. Provided by the Dashboard, which owns the
 * view, so a tip on the home screen and the same tip listed in Settings both work. Kept in a
 * component-free module so Fast Refresh doesn't recreate the context.
 */
export interface TipNav {
  navigate: (view: DashboardView) => void;
}

export const TipNavContext = createContext<TipNav>({ navigate: () => {} });

export function useTipNav(): TipNav {
  return useContext(TipNavContext);
}
