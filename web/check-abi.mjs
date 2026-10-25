// Drive the module exactly like web/index.html does, to catch ABI drift.
import { readFileSync } from "node:fs";

const wasmPath = process.argv[2] ?? "web/libresvip_web.wasm";
const inputPath = process.argv[3] ?? "testdata/simple.json";

const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
const api = instance.exports;

const encoder = new TextEncoder();
const decoder = new TextDecoder();
function writeString(text) {
  const bytes = encoder.encode(text);
  const pointer = api.alloc(bytes.length);
  new Uint8Array(api.memory.buffer, pointer, bytes.length).set(bytes);
  return { pointer, length: bytes.length };
}
const read = () => new Uint8Array(api.memory.buffer, api.result_ptr(), api.result_len()).slice();

api.formats_json();
const formats = JSON.parse(decoder.decode(read()));
console.log("formats:", formats.length);
const withOptions = formats.filter((format) => format.features.length);
console.log("formats with features:", withOptions.length, "e.g.", withOptions[0]?.id, withOptions[0]?.features.join(","));
console.log("single-track formats:", formats.filter((format) => format.max_tracks_per_file === 1).length);

const source = writeString("json");
const target = writeString("ustx");
api.options_json(source.pointer, source.length, target.pointer, target.length);
const options = JSON.parse(decoder.decode(read()));
console.log("options json:", Object.keys(options), "input fields:", options.input.length, "output fields:", options.output.length);

const input = readFileSync(inputPath);
const pointer = api.alloc(input.length);
new Uint8Array(api.memory.buffer, pointer, input.length).set(input);
const empty = writeString("{}");
const status = api.convert_with_options(
  pointer, input.length,
  source.pointer, source.length,
  target.pointer, target.length,
  empty.pointer, empty.length,
  empty.pointer, empty.length,
);
const payload = read();
api.warnings_json();
const warnings = JSON.parse(decoder.decode(read()));
console.log("convert:", status === 0 ? `ok ${payload.length} bytes` : `failed ${decoder.decode(payload)}`);
console.log("warnings:", warnings.length, warnings.slice(0, 2));
