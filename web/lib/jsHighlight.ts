// Minimal JavaScript highlighter for the code workspace overlay. Returns HTML with <span class="hl-*"> tokens.
// Escapes everything first, so source text can never inject markup.

const KW = new Set("break case catch class const continue default delete do else export extends finally for function if import in instanceof let new return switch this throw try typeof var void while yield async await of null undefined true false".split(" "));

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

export function highlightJs(src: string): string {
  const re = /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)|("(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*'|`(?:\\.|[^`\\])*`)|(\b\d[\d_]*(?:\.\d+)?\b)|([A-Za-z_$][\w$]*)(\s*\()?/g;
  let out = "";
  let last = 0;
  for (let m = re.exec(src); m; m = re.exec(src)) {
    out += esc(src.slice(last, m.index));
    if (m[1]) out += `<span class="hl-c">${esc(m[1])}</span>`;
    else if (m[2]) out += `<span class="hl-s">${esc(m[2])}</span>`;
    else if (m[3]) out += `<span class="hl-n">${esc(m[3])}</span>`;
    else if (m[4]) {
      const w = m[4];
      const cls = KW.has(w) ? "hl-k" : m[5] ? "hl-f" : w === "ctx" || w === "console" ? "hl-v" : "";
      out += cls ? `<span class="${cls}">${esc(w)}</span>` : esc(w);
      if (m[5]) out += esc(m[5]);
    }
    last = re.lastIndex;
  }
  out += esc(src.slice(last));
  // A trailing newline needs a visible line so the overlay keeps the textarea's height.
  return out.endsWith("\n") ? `${out} ` : out;
}
