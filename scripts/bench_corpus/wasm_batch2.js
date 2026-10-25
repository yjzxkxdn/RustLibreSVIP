// Warm-path harness for the WebAssembly module.
//
//   node wasm_batch2.js <jobs.tsv> <output dir>
//
// The jobs file has `input<TAB>output<TAB>source-format-id<TAB>target-format-id`
// per line (the same file `phases --jobs` reads). One module instance serves the
// whole file, so this measures what a browser tab pays for the second, third and
// nth conversion -- instantiate happens once, at the start.
import { readFileSync, writeFileSync, statSync } from "node:fs";
import path from "node:path";

const [, , jobsPath, outDir] = process.argv;
if (!jobsPath || !outDir) {
  console.error("usage: node wasm_batch2.js <jobs.tsv> <output dir>");
  process.exit(2);
}
const wasmPath = process.env.WASM_MODULE;
if (!wasmPath) {
  console.error("WASM_MODULE is not set");
  process.exit(2);
}

const loadStarted = performance.now();
const bytes = readFileSync(path.resolve(wasmPath));
const { instance } = await WebAssembly.instantiate(bytes, {});
const api = instance.exports;
const instantiateMs = performance.now() - loadStarted;

function writeString(text) {
  const encoded = new TextEncoder().encode(text);
  const pointer = api.alloc(encoded.length);
  new Uint8Array(api.memory.buffer, pointer, encoded.length).set(encoded);
  return { pointer, length: encoded.length };
}

function convert(input, outPath, srcFormat, dstFormat) {
  const inputPointer = api.alloc(input.length);
  new Uint8Array(api.memory.buffer, inputPointer, input.length).set(input);
  const source = writeString(srcFormat);
  const target = writeString(dstFormat);
  const started = performance.now();
  const status = api.convert(
    inputPointer, input.length,
    source.pointer, source.length,
    target.pointer, target.length,
  );
  const convertMs = performance.now() - started;
  const payload = new Uint8Array(api.memory.buffer, api.result_ptr(), api.result_len()).slice();
  api.dealloc(inputPointer, input.length);
  api.dealloc(source.pointer, source.length);
  api.dealloc(target.pointer, target.length);
  if (status !== 0) {
    return { ok: false, convert_ms: convertMs, error: Buffer.from(payload).toString("utf8").slice(0, 200) };
  }
  writeFileSync(outPath, payload);
  return { ok: true, convert_ms: convertMs, bytes: statSync(outPath).size };
}

const jobs = readFileSync(jobsPath, "utf8")
  .split(/\r?\n/)
  .map((line) => line.trim())
  .filter((line) => line.length > 0)
  .map((line) => line.split("\t"));

const memoryBefore = api.memory.buffer.byteLength;
let index = 0;
let failures = 0;
const startedAll = performance.now();
for (const [inputPath, outputPath, srcFormat, dstFormat] of jobs) {
  const input = readFileSync(inputPath);
  const result = convert(input, outputPath, srcFormat, dstFormat);
  const row = {
    i: index,
    ok: result.ok,
    convert_ms: result.convert_ms,
    bytes: result.bytes ?? 0,
    // linear memory never shrinks; reporting it shows what a long session costs
    memory_mb: api.memory.buffer.byteLength / 1048576,
  };
  if (!result.ok) {
    row.error = result.error;
    failures += 1;
  }
  console.log(JSON.stringify(row));
  index += 1;
}
console.log(JSON.stringify({
  jobs: jobs.length,
  failures,
  instantiate_ms: instantiateMs,
  memory_before_mb: memoryBefore / 1048576,
  memory_after_mb: api.memory.buffer.byteLength / 1048576,
  total_ms: performance.now() - startedAll,
}));
process.exit(0);
