// Node harness around the RustLibreSVIP WebAssembly module.
//
//   node wasm_cli.js <input> <output> <source-format-id> <target-format-id>
//
// Prints one JSON line with the in-process conversion time. The module is
// instantiated once per process, so `load_ms` is the one-off cost a browser pays
// when the page opens and `convert_ms` is what a conversion actually costs.
import { readFileSync, writeFileSync, statSync } from "node:fs";
import path from "node:path";

const [, , srcPath, outPath, srcFormat, outFormat] = process.argv;
if (!srcPath || !outPath || !srcFormat || !outFormat) {
  console.error("usage: node wasm_cli.js <input> <output> <source-format> <target-format>");
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
const loadMs = performance.now() - loadStarted;

function writeString(text) {
  const encoded = new TextEncoder().encode(text);
  const pointer = api.alloc(encoded.length);
  new Uint8Array(api.memory.buffer, pointer, encoded.length).set(encoded);
  return { pointer, length: encoded.length };
}

const input = readFileSync(srcPath);
const inputPointer = api.alloc(input.length);
new Uint8Array(api.memory.buffer, inputPointer, input.length).set(input);
const source = writeString(srcFormat);
const target = writeString(outFormat);

const convertStarted = performance.now();
const status = api.convert(
  inputPointer,
  input.length,
  source.pointer,
  source.length,
  target.pointer,
  target.length,
);
const convertMs = performance.now() - convertStarted;

const resultPointer = api.result_ptr();
const resultLength = api.result_len();
const payload = new Uint8Array(api.memory.buffer, resultPointer, resultLength).slice();
api.dealloc(inputPointer, input.length);
api.dealloc(source.pointer, source.length);
api.dealloc(target.pointer, target.length);


if (status === 0) {
  writeFileSync(outPath, payload);
} else {
  console.error(Buffer.from(payload).toString("utf8").slice(0, 300));
}

console.log(JSON.stringify({
  ok: status === 0,
  convert_ms: convertMs,
  load_ms: loadMs,
  bytes: status === 0 ? statSync(outPath).size : 0,
}));
process.exit(status === 0 ? 0 : 1);
