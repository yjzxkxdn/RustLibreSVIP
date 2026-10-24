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
import { spawnSync } from "node:child_process";

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

// Optional binaryen post-processing: when `wasm-opt` is on the PATH the module
// is shrunk with `-Oz` first; otherwise the cargo output is packaged as-is.
// Pass --skip-opt to disable even when binaryen is installed.
let optimizedPath = modulePath;
if (!process.argv.includes("--skip-opt")) {
  const probe = spawnSync("wasm-opt", ["--version"], { encoding: "utf8" });
  if (probe.status === 0) {
    const out = modulePath.replace(/\.wasm$/, ".opt.wasm");
    const run = spawnSync(
      "wasm-opt",
      [modulePath, "-Oz", "--enable-bulk-memory", "-o", out],
      { encoding: "utf8" },
    );
    if (run.status === 0) {
      console.log(`wasm-opt -Oz: ${readFileSync(modulePath).length} -> ${readFileSync(out).length} bytes`);
      optimizedPath = out;
    } else {
      console.warn(`wasm-opt failed (status ${run.status}), using the unoptimized module`);
      console.warn(run.stderr);
    }
  } else {
    console.log("wasm-opt not found on PATH; skipping the binaryen pass (install binaryen to enable)");
  }
}

const wasm = readFileSync(optimizedPath);
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
