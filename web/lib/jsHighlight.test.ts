import { describe, expect, it } from "vitest";
import { highlightJs } from "./jsHighlight";

describe("highlightJs", () => {
  it("tags keywords, strings, numbers, comments and calls", () => {
    const h = highlightJs('function decide(ctx) { return "x" + 2; } // hi');
    expect(h).toContain('<span class="hl-k">function</span>');
    expect(h).toContain('<span class="hl-f">decide</span>(');
    expect(h).toContain('<span class="hl-v">ctx</span>');
    expect(h).toContain('<span class="hl-s">"x"</span>');
    expect(h).toContain('<span class="hl-n">2</span>');
    expect(h).toContain('<span class="hl-c">// hi</span>');
  });
  it("escapes markup in source", () => {
    expect(highlightJs("a < b && '<img>'")).not.toContain("<img>");
    expect(highlightJs("x<y")).toContain("&lt;");
  });
});
