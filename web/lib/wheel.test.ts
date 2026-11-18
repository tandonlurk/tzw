import { describe, expect, it } from "vitest";
import { arcPath, fitLabel, polar, subcategoriesOf, sunburstLayout } from "./wheel";

describe("wheel geometry", () => {
  it("polar is clockwise from 12 o'clock", () => {
    const top = polar(0, 0, 10, 0);
    expect(top.x).toBeCloseTo(0);
    expect(top.y).toBeCloseTo(-10);
    const right = polar(0, 0, 10, Math.PI / 2);
    expect(right.x).toBeCloseTo(10);
    expect(right.y).toBeCloseTo(0);
  });

  it("lays out categories and their subcategories", () => {
    const { inner, outer } = sunburstLayout(["Crypto", "Commodities"], { commodities: ["Energy", "Metals"] });
    expect(inner.map((s) => [s.label, s.a0, s.a1])).toEqual([["Crypto", 0, Math.PI], ["Commodities", Math.PI, 2 * Math.PI]]);
    expect(outer.map((s) => [s.label, s.parent])).toEqual([["Energy", "Commodities"], ["Metals", "Commodities"]]);
    expect(outer[0].a0).toBeCloseTo(Math.PI);
    expect(outer[1].a1).toBeCloseTo(2 * Math.PI);
    expect(sunburstLayout([], {})).toEqual({ inner: [], outer: [] });
    expect(subcategoriesOf({ Crypto: ["BTC"] }, "crypto")).toEqual(["BTC"]);
  });

  it("draws sectors and full rings", () => {
    expect(arcPath(0, 0, 5, 10, 0, Math.PI / 2)).toMatch(/^M 0 -10 A 10 10 0 0 1 10 0 L 5 0 A 5 5 0 0 0 0 -5 Z$/);
    expect(arcPath(0, 0, 5, 10, 0, Math.PI * 2).match(/M /g)).toHaveLength(2);
  });

  it("fits labels", () => {
    expect(fitLabel("Commodities", 6)).toBe("Commo…");
    expect(fitLabel("BTC", 6)).toBe("BTC");
  });
});
