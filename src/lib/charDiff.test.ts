import { describe, expect, it } from "vitest";
import { charDiff, side, visible } from "./charDiff";

describe("charDiff", () => {
  it("returns one 'same' part for equal values and nothing for two empties", () => {
    expect(charDiff("fake_value_1", "fake_value_1")).toEqual([{ type: "same", text: "fake_value_1" }]);
    expect(charDiff("", "")).toEqual([]);
  });

  it("pinpoints a single changed character", () => {
    expect(charDiff("postgres://prod-db/app", "postgres://prod-dc/app")).toEqual([
      { type: "same", text: "postgres://prod-d" },
      { type: "del", text: "b" },
      { type: "add", text: "c" },
      { type: "same", text: "/app" },
    ]);
  });

  it("finds a trailing slash or space", () => {
    expect(charDiff("redis://x.test/", "redis://x.test")).toEqual([
      { type: "same", text: "redis://x.test" },
      { type: "del", text: "/" },
    ]);
    expect(charDiff("value ", "value")).toEqual([
      { type: "same", text: "value" },
      { type: "del", text: " " },
    ]);
  });

  it("handles one side empty", () => {
    expect(charDiff("", "fake")).toEqual([{ type: "add", text: "fake" }]);
    expect(charDiff("fake", "")).toEqual([{ type: "del", text: "fake" }]);
  });

  it("always rebuilds both sides exactly (property check)", () => {
    let seed = 7;
    const rnd = () => {
      seed = (seed * 1103515245 + 12345) % 2147483648;
      return seed / 2147483648;
    };
    const alphabet = "ab/:.-_ é1";
    const word = () => Array.from({ length: Math.floor(rnd() * 14) }, () => alphabet[Math.floor(rnd() * alphabet.length)]).join("");
    for (let i = 0; i < 400; i++) {
      const a = word();
      const b = word();
      const parts = charDiff(a, b);
      expect(side(parts, "left")).toBe(a);
      expect(side(parts, "right")).toBe(b);
      // Adjacent parts never share a type (they are merged).
      for (let k = 1; k < parts.length; k++) expect(parts[k].type).not.toBe(parts[k - 1].type);
    }
  });

  it("does not blow up on very long values", () => {
    const a = "x".repeat(3000) + "A".repeat(1000);
    const b = "x".repeat(3000) + "B".repeat(1000);
    const t0 = performance.now();
    const parts = charDiff(a, b);
    expect(performance.now() - t0).toBeLessThan(500);
    expect(side(parts, "left")).toBe(a);
    expect(side(parts, "right")).toBe(b);
  });

  it("works on emoji and other multi-unit characters", () => {
    const parts = charDiff("key🔑a", "key🔑b");
    expect(side(parts, "left")).toBe("key🔑a");
    expect(parts[0]).toEqual({ type: "same", text: "key🔑" });
  });
});

describe("visible", () => {
  it("shows whitespace that would otherwise be invisible", () => {
    expect(visible("  a b\t\n ")).toBe("··a b→↵·");
  });
});
