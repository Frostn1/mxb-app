import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { runAction } from "./actions";

describe("runAction", () => {
  it("applies the optimistic change before the work finishes", async () => {
    const order: string[] = [];
    let release!: () => void;
    const pending = runAction({
      name: "t",
      optimistic: () => { order.push("optimistic"); return () => order.push("undo"); },
      run: () => new Promise<void>((resolve) => { order.push("run"); release = resolve; }),
    });
    expect(order).toEqual(["optimistic", "run"]);
    release();
    expect((await pending).ok).toBe(true);
    expect(order).toEqual(["optimistic", "run"]);
  });

  it("undoes the change and reports the error when the work fails", async () => {
    const undo = vi.fn();
    const result = await runAction({ name: "t", optimistic: () => undo, run: () => Promise.reject("boom") });
    expect(undo).toHaveBeenCalledOnce();
    expect(result).toMatchObject({ ok: false, error: "boom" });
  });
});