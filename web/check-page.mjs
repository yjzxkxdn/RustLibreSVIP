// Static sanity check for web/index.html: every $("#id") / getElementById target
// must exist in the markup, and every id must be unique.
import { readFileSync } from "node:fs";

const html = readFileSync(process.argv[2] ?? "web/index.html", "utf8");
const ids = [...html.matchAll(/\sid="([^"]+)"/g)].map((match) => match[1]);
const duplicates = ids.filter((id, index) => ids.indexOf(id) !== index);
const referenced = new Set();
for (const match of html.matchAll(/\$\("#([A-Za-z0-9_-]+)"\)/g)) referenced.add(match[1]);
for (const match of html.matchAll(/querySelector\("#([A-Za-z0-9_-]+)"\)/g)) referenced.add(match[1]);
for (const match of html.matchAll(/getElementById\("([A-Za-z0-9_-]+)"\)/g)) referenced.add(match[1]);
const missing = [...referenced].filter((id) => !ids.includes(id));
console.log("ids:", ids.length, "referenced:", referenced.size);
console.log("duplicate ids:", duplicates.length ? duplicates : "none");
console.log("missing ids:", missing.length ? missing : "none");
const templates = [...html.matchAll(/id="([^"]*\$\{[^"]*)"/g)];
console.log("dynamic ids:", templates.length ? templates.map((m) => m[1]) : "none");
process.exit(missing.length || duplicates.length ? 1 : 0);
