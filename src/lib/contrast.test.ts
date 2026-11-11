// WCAG AA check for every text/background pair the UI uses, in both themes. Reads the real tokens
// from index.css so a token change that breaks contrast fails the build.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");

function tokens(block: RegExp): Record<string, string> {
  const m = css.match(block);
  if (!m) throw new Error("token block not found");
  return Object.fromEntries([...m[1].matchAll(/--([a-z]+):\s*(#[0-9a-fA-F]{6})/g)].map((x) => [x[1], x[2]]));
}

const dark = tokens(/:root\s*\{([^}]*)\}/);
const light = tokens(/:root\[data-theme="light"\]\s*\{([^}]*)\}/);

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5]
    .map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

// [foreground, background] pairs actually rendered as text.
const PAIRS: [string, string][] = [
  ["chalk", "ink"],
  ["chalk", "surface"],
  ["muted", "ink"],
  ["muted", "surface"],
  ["prod", "ink"],
  ["prod", "surface"],
  ["staging", "ink"],
  ["staging", "surface"],
  ["dev", "ink"],
  ["dev", "surface"],
  ["focus", "ink"],
  ["focus", "surface"],
  ["ink", "focus"], // primary button label
];

describe("WCAG AA contrast (4.5:1)", () => {
  it("parses both token sets", () => {
    expect(Object.keys(dark)).toContain("ink");
    expect(Object.keys(light)).toContain("ink");
  });
  for (const [name, theme] of [
    ["dark", dark],
    ["light", light],
  ] as const) {
    for (const [fg, bg] of PAIRS) {
      it(`${name}: ${fg} on ${bg}`, () => {
        expect(contrast(theme[fg], theme[bg])).toBeGreaterThanOrEqual(4.5);
      });
    }
  }
});
