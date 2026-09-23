import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import RetainedView, {
  RETAIN_IDLE_MS,
  retainAfterVisit,
  slotsWhileActive,
} from "../src/Components/Shell/RetainedView";

test("a retained view mounts lazily and remains hot during its idle window", () => {
  let visited = false;
  visited = retainAfterVisit(visited, false);
  expect(visited).toBe(false);
  visited = retainAfterVisit(visited, true);
  expect(visited).toBe(true);
  visited = retainAfterVisit(visited, false);
  expect(visited).toBe(true);
  expect(RETAIN_IDLE_MS).toBe(300_000);
});

test("an unvisited view renders nothing and an active view renders its subtree", () => {
  const slots = { left: null, right: null };
  expect(
    renderToStaticMarkup(
      <RetainedView active={false} slots={slots}>
        <span>expensive grid</span>
      </RetainedView>,
    ),
  ).toBe("");

  const active = renderToStaticMarkup(
    <RetainedView active slots={slots}>
      <span>expensive grid</span>
    </RetainedView>,
  );
  expect(active).toContain('data-view-active="true"');
  expect(active).toContain("expensive grid");
});

test("hidden retained views are disconnected from the context bar", () => {
  const left = {} as HTMLElement;
  const right = {} as HTMLElement;
  const slots = { left, right };
  expect(slotsWhileActive(true, slots)).toBe(slots);
  expect(slotsWhileActive(false, slots)).toEqual({ left: null, right: null });
});
