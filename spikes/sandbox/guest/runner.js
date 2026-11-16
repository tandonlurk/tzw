// Sandbox runner guest, compiled ONCE with Javy into runner.wasm.
//
// stdin:  {"code": "<user JS defining decide(ctx)>", "ctx": {...}, "now_ms": <int>, "seed": <int>}
// stdout: {"ok": true, "intents": <whatever decide returned>, "logs": [...]}
//      or {"ok": false, "error": "..."}
//
// The host validates everything this writes; nothing here is trusted. The
// point of this file is only to make user code deterministic and to keep it
// away from the raw I/O builtins.

const IO = Javy.IO;
const stringify = JSON.stringify;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function readStdin() {
  const chunks = [];
  let total = 0;
  for (;;) {
    const buf = new Uint8Array(4096);
    const n = IO.readSync(0, buf);
    if (n === 0) break;
    chunks.push(buf.subarray(0, n));
    total += n;
  }
  const all = new Uint8Array(total);
  let off = 0;
  for (const c of chunks) {
    all.set(c, off);
    off += c.length;
  }
  return decoder.decode(all);
}

function writeStdout(obj) {
  IO.writeSync(1, encoder.encode(stringify(obj)));
}

const logs = [];
const MAX_LOGS = 20;
const MAX_LOG_LEN = 200;
function log(...args) {
  if (logs.length < MAX_LOGS) logs.push(args.map(String).join(" ").slice(0, MAX_LOG_LEN));
}

let input;
try {
  input = JSON.parse(readStdin());
} catch (e) {
  writeStdout({ ok: false, error: "bad input: " + String(e) });
  throw e;
}

// Determinism: time comes only from the host-supplied now_ms, randomness only
// from a seeded PRNG (mulberry32). The host's WASI clock/random stubs are
// fixed as well; this layer makes the JS-visible values meaningful.
const NOW = Number(input.now_ms) || 0;
const RealDate = Date;
function FixedDate(...a) {
  if (!new.target) return new RealDate(NOW).toString();
  return a.length ? new RealDate(...a) : new RealDate(NOW);
}
FixedDate.now = () => NOW;
FixedDate.parse = RealDate.parse;
FixedDate.UTC = RealDate.UTC;
FixedDate.prototype = RealDate.prototype;
globalThis.Date = FixedDate;

let seed = (Number(input.seed) | 0) >>> 0;
Math.random = function () {
  seed = (seed + 0x6d2b79f5) >>> 0;
  let t = seed;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};

globalThis.console = { log, info: log, warn: log, error: log, debug: log };
// User code must not reach the raw stdin/stdout builtins.
delete globalThis.Javy;

try {
  const run = new Function(
    "ctx",
    String(input.code) +
      "\n;if (typeof decide !== 'function') throw new Error('decide(ctx) is not defined');\nreturn decide(ctx);"
  );
  const intents = run(input.ctx);
  writeStdout({ ok: true, intents, logs });
} catch (e) {
  writeStdout({ ok: false, error: String(e && e.message ? e.message : e).slice(0, 500), logs });
}
