// Packages the web demo from a built module:
//
//   cargo build --profile wasm -p libresvip-web --target wasm32-unknown-unknown
//   node web/build-web.mjs
//
// Produces, next to this file:
//   libresvip_web.wasm       the module (serve it with Content-Type application/wasm)
//   libresvip-single.html    the same page with the gzip-compressed module inlined,
//                            so it can be opened straight from disk
//
// No bundler, no wasm-bindgen: the module has no imports and `index.html` drives
// it through the C ABI documented in crates/libresvip-web/src/lib.rs.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const here = dirname(fileURLToPath(import.meta.url));
const repository = resolve(here, "..");
const defaultModule = join(
  repository,
  "target",
  "wasm32-unknown-unknown",
  "wasm",
  "libresvip_web.wasm",
);
const modulePath = process.argv[2] ?? defaultModule;

const wasm = readFileSync(modulePath);
const page = readFileSync(join(here, "index.html"), "utf8");

writeFileSync(join(here, "libresvip_web.wasm"), wasm);

const compressed = gzipSync(wasm, { level: 9 });
const inline = `<script>window.__LIBRESVIP_WASM_GZIP_BASE64__ = "${compressed.toString("base64")}";</script>\n`;
const single = page.replace('<script>\n"use strict";', `${inline}<script>\n"use strict";`);
if (single === page) {
  throw new Error("could not find the inline script anchor in web/index.html");
}
writeFileSync(join(here, "libresvip-single.html"), single);

const kib = (bytes) => `${(bytes / 1024).toFixed(0)} KiB`;
console.log(`module      ${kib(wasm.length)} (${wasm.length} bytes)`);
console.log(`gzip        ${kib(compressed.length)}`);
console.log(`single file ${kib(single.length)}`);
