import { describe, expect, it } from "vitest";
import { bootstrap } from "./main";

describe("bootstrap", () => {
  it("runs without throwing", () => {
    expect(() => {
      bootstrap();
    }).not.toThrow();
  });
});
