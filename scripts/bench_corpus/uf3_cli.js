// Node harness around the UtaFormatix3 Kotlin/JS library.
//
//   node uf3_cli.js <input> <output> <source-ext> <target-format>
//
// Prints one JSON line with the in-process conversion time so the benchmark can
// separate "Node + module startup" from the actual parse/generate work.
import "./uf3_polyfill.mjs";
import { readFileSync, writeFileSync, statSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const [, , srcPath, outPath, srcExt, target] = process.argv;
if (!srcPath || !outPath || !srcExt || !target) {
  console.error("usage: node uf3_cli.js <input> <output> <source-ext> <target>");
  process.exit(2);
}

const modulePath = process.env.UF3_MODULE;
if (!modulePath) {
  console.error("UF3_MODULE is not set");
  process.exit(2);
}

const PARSE = {
  ustx: "parseUstx",
  svp: "parseSvp",
  vsqx: "parseVsqx",
  vsq: "parseVsq",
  ust: "parseUst",
  ccs: "parseCcs",
  s5p: "parseS5p",
  musicxml: "parseMusicXml",
  mxl: "parseMusicXml",
  dv: "parseDv",
  vpr: "parseVpr",
  ppsf: "parsePpsf",
  ufdata: "parseUfData",
  tssln: "parseTssln",
};

const GENERATE = {
  ustx: "generateUstx",
  svp: "generateSvp",
  vsqx: "generateVsqx",
  vsq: "generateVsq",
  ust: "generateUstZip",
  ccs: "generateCcs",
  s5p: "generateS5p",
  musicxml: "generateMusicXmlZip",
  dv: "generateDv",
  vpr: "generateVpr",
  tssln: "generateTssln",
  ufdata: "generateUfData",
};

const started = performance.now();
const imported = await import(pathToFileURL(path.resolve(modulePath)).href);
// ESM bundles expose named exports; webpack UMD bundles hang them off `default`.
const api = { ...(imported.default && typeof imported.default === "object" ? imported.default : {}), ...imported };
const loadMs = performance.now() - started;

const parseName = PARSE[srcExt];
const generateName = GENERATE[target];
const missing = [parseName, generateName].filter((name) => typeof api[name] !== "function");
if (!parseName || !generateName || missing.length) {
  console.error(`unsupported: ${srcExt} -> ${target} (missing ${missing.join(",") || "mapping"})`);
  process.exit(3);
}

const parseStarted = performance.now();
const bytes = readFileSync(srcPath);
const file = new File([bytes], path.basename(srcPath));
const importParams = new api.ImportParams(false, false, "la");
// LibreSVIP always converts pitch, so enable the equivalent feature here.
const conversionParams = new api.ConversionParams(true);

let container;
if (srcExt === "ust") {
  container = await api.parseUst([file], importParams);
} else {
  container = await api[parseName](file, importParams);
}
const parseMs = performance.now() - parseStarted;

const generateStarted = performance.now();
const result = await api[generateName](container, conversionParams);
const generateMs = performance.now() - generateStarted;

const outBytes = Buffer.from(await result.blob.arrayBuffer());
writeFileSync(outPath, outBytes);
console.log(JSON.stringify({
  inner_ms: parseMs + generateMs,
  parse_ms: parseMs,
  generate_ms: generateMs,
  load_ms: loadMs,
  bytes: statSync(outPath).size,
  fileName: result.fileName,
}));
