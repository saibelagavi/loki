import { describe, expect, it } from "vitest";
import { passwordStrength } from "./strength";

describe("passwordStrength", () => {
  it("asks for input when empty", () => {
    expect(passwordStrength("").score).toBe(0);
    expect(passwordStrength("").label).toBe("");
  });
  it("flags short passwords", () => {
    expect(passwordStrength("abc123").label).toBe("Too short");
  });
  it("penalizes common words and repeats", () => {
    expect(passwordStrength("password1234").score).toBeLessThanOrEqual(1);
    expect(passwordStrength("aaaaaaaaaaaa").score).toBeLessThanOrEqual(1);
  });
  it("rewards length and variety", () => {
    const weak = passwordStrength("horsebat");
    const strong = passwordStrength("correct-Horse-battery-9-staple");
    expect(strong.score).toBeGreaterThan(weak.score);
    expect(strong.score).toBeGreaterThanOrEqual(3);
  });
  it("never returns a score outside 0..4", () => {
    for (const p of ["", "a", "Abcdefg1!", "x".repeat(200), "Tr0ub4dor&3-Tr0ub4dor&3-Tr0ub4dor&3"]) {
      const s = passwordStrength(p).score;
      expect(s).toBeGreaterThanOrEqual(0);
      expect(s).toBeLessThanOrEqual(4);
    }
  });
});
