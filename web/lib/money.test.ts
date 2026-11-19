import { describe, expect, it } from "vitest";
import { formatMoney, parseMoney, signOf, sumMoney } from "./money";

describe("formatMoney", () => {
  it("formats whole and fractional amounts", () => {
    expect(formatMoney("1500000")).toBe("1.50 USDC");
    expect(formatMoney("0", { unit: false })).toBe("0.00");
    expect(formatMoney("1")).toBe("0.000001 USDC");
    expect(formatMoney("123456789012345678", { unit: false })).toBe("123,456,789,012.345678");
  });
  it("handles sign", () => {
    expect(formatMoney("-2500000", { unit: false })).toBe("-2.50");
    expect(formatMoney("2500000", { signed: true, unit: false })).toBe("+2.50");
    expect(formatMoney("0", { signed: true, unit: false })).toBe("0.00");
  });
  it("is exact beyond float precision", () => {
    expect(formatMoney("9007199254740993", { unit: false })).toBe("9,007,199,254.740993");
  });
  it("returns n/a for garbage", () => {
    expect(formatMoney("abc")).toBe("n/a");
  });
});

describe("parseMoney", () => {
  it("parses to minor units", () => {
    expect(parseMoney("1.5")).toBe("1500000");
    expect(parseMoney("10")).toBe("10000000");
    expect(parseMoney("0.000001")).toBe("1");
    expect(parseMoney(" 1,000.25 ")).toBe("1000250000");
    expect(parseMoney("1.")).toBe("1000000");
  });
  it("rejects invalid input", () => {
    expect(parseMoney("")).toBeNull();
    expect(parseMoney("-1")).toBeNull();
    expect(parseMoney("1.1234567")).toBeNull();
    expect(parseMoney("abc")).toBeNull();
    expect(parseMoney("1e6")).toBeNull();
    expect(parseMoney(".5")).toBeNull();
  });
  it("round-trips", () => {
    for (const v of ["0", "1", "999999", "1000000", "123456789"]) {
      expect(parseMoney(formatMoney(v, { unit: false, minFrac: 6 }))).toBe(v);
    }
  });
});

describe("sumMoney / signOf", () => {
  it("sums exactly", () => {
    expect(sumMoney(["9007199254740993", "1", "-4"])).toBe("9007199254740990");
  });
  it("sign", () => {
    expect(signOf("-1")).toBe(-1);
    expect(signOf("0")).toBe(0);
    expect(signOf("5")).toBe(1);
  });
});

describe("parseMoney leniency", () => {
  it("accepts currency symbols, separators and a unit suffix", () => {
    expect(parseMoney("$1,000.50")).toBe("1000500000");
    expect(parseMoney("1 000.5 USDC")).toBe("1000500000");
    expect(parseMoney("$-1")).toBe(null);
  });
});
