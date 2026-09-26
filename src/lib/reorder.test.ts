import { describe, expect, it } from "vitest";
import { moveId } from "./reorder";

describe("moveId", () => {
  it("moves the dragged id to the front", () => {
    const result = moveId(["a", "b", "c"], "c", "a");
    expect(result).toEqual(["c", "a", "b"]);
  });

  it("moves the dragged id to the end", () => {
    const result = moveId(["a", "b", "c"], "a", "c");
    expect(result).toEqual(["b", "c", "a"]);
  });

  it("returns the original array reference when dropped on its own position", () => {
    const ids = ["a", "b", "c"];
    const result = moveId(ids, "b", "b");
    expect(result).toBe(ids);
  });

  it("returns the original array reference when either id is unknown", () => {
    const ids = ["a", "b", "c"];
    expect(moveId(ids, "z", "a")).toBe(ids);
    expect(moveId(ids, "a", "z")).toBe(ids);
  });
});
