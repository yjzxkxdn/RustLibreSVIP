// Boot the single-file page inside jsdom and report what the UI ended up with.
// Usage: node check-ui.mjs <path-to-libresvip-single.html>
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { JSDOM, VirtualConsole } = require("jsdom");

const page = process.argv[2];
const html = readFileSync(page, "utf8");
const logs = [];
const virtualConsole = new VirtualConsole();
virtualConsole.on("error", (e) => logs.push(`console.error: ${e}`));
virtualConsole.on("log", (m) => logs.push(`console.log: ${m}`));
virtualConsole.on("jsdomError", (e) => logs.push(`jsdomError: ${e.message}`));

const dom = new JSDOM(html, {
  runScripts: "dangerously",
  pretendToBeVisual: true,
  url: "file:///page/libresvip-single.html",
  virtualConsole,
  beforeParse(window) {
    window.WebAssembly = WebAssembly;
    window.DecompressionStream = DecompressionStream;
    // jsdom's Blob has no .stream() and no Response; use Node's.
    window.Blob = Blob;
    window.Response = Response;
    window.fetch = () => Promise.reject(new Error("no network in this check"));
    // jsdom has neither of these; the page only needs them to exist.
    window.matchMedia = (query) => ({
      media: query,
      matches: false,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
    });
    window.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  },
});

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function main() {
  for (let i = 0; i < 60 && !dom.window.document.querySelector("#input-format").options.length; i++) {
    await wait(100);
  }
  await wait(1500);
  const doc = dom.window.document;
  const text = (sel) => doc.querySelector(sel)?.textContent?.trim() ?? "<missing>";
  const status = text("#status");
  const badge = text("#option-badge");
  const inputFields = doc.querySelectorAll('.fields[data-side="input"] [data-key]').length;
  const outputFields = doc.querySelectorAll('.fields[data-side="output"] [data-key]').length;
  const keyedWithoutSide = [...doc.querySelectorAll(".fields [data-key]")].filter(
    (el) => !["input", "output"].includes(el.dataset.side),
  ).length;
  console.log("input fmt :", text("#input-format") || "<empty>");
  console.log("output fmt:", text("#output-format") || "<empty>");
  console.log("fields    : input", inputFields, "output", outputFields, "without side", keyedWithoutSide);
  console.log("badge     :", badge);
  console.log("status    :", status);

  // Toggling a switch must not throw and must flip the badge.
  const toggle = doc.querySelector('.fields[data-side="input"] input[type="checkbox"]');
  let collected = null;
  let badgeAfter = "n/a";
  if (toggle) {
    toggle.checked = !toggle.checked;
    toggle.dispatchEvent(new dom.window.Event("change", { bubbles: true }));
    badgeAfter = text("#option-badge");
    collected = dom.window.collectOptions();
  }
  console.log("after flip:", badgeAfter, "collected input keys:", collected ? Object.keys(collected.input).length : "n/a");

  const problems = logs.filter((line) => /Cannot set properties|Failed to fetch|TypeError/.test(line));
  console.log("problems  :", problems.length ? problems.slice(0, 3).join(" | ") : "none");

  const failed =
    problems.length > 0 ||
    keyedWithoutSide > 0 ||
    !status.includes("模块已加载") ||
    inputFields === 0 ||
    outputFields === 0 ||
    badge !== "默认" ||
    (toggle && !badgeAfter.includes("1 项已改")) ||
    (toggle && (!collected || Object.keys(collected.input).length === 0));
  dom.window.close();
  if (failed) {
    console.error("UI CHECK FAILED");
    process.exit(1);
  }
  console.log("UI CHECK OK");
}

main().catch((error) => {
  console.error("harness failed:", error.message);
  process.exit(1);
});
