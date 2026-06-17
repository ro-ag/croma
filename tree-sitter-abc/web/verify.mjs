// verify.mjs — HEADLESS web-reuse proof (no browser).
//
// Loads web-tree-sitter (0.26.x, the same runtime the browser demo uses),
// instantiates the ABC grammar from the committed `tree-sitter-abc.wasm`,
// parses a sample tune, runs the canonical `queries/highlights.scm`, and
// ASSERTS:
//   1. the parse tree's root has no ERROR  (tree.rootNode.hasError === false)
//   2. the highlight query yields > 0 captures
//
// Exits 0 on success (printing the capture count + error state), non-zero on
// failure. This is the automatable gate behind `npm run verify:web` — it is the
// headless equivalent of opening index.html and seeing the sample highlighted.
//
// Run:  node web/verify.mjs        (from the tree-sitter-abc/ package root)
//   or: npm run verify:web

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
// Bare specifier — resolved from node_modules after `npm install` (web-tree-sitter
// is a declared devDependency). node_modules stays git-ignored; this script is
// the reproducible gate run on top of an install.
import { Parser, Language, Query } from "web-tree-sitter";

const here = dirname(fileURLToPath(import.meta.url)); // tree-sitter-abc/web
const pkgRoot = join(here, ".."); // tree-sitter-abc

const GRAMMAR_WASM = join(pkgRoot, "tree-sitter-abc.wasm");
const HIGHLIGHTS = join(pkgRoot, "queries", "highlights.scm");
const RUNTIME_WASM = join(here, "vendor", "web-tree-sitter.wasm");

// A small but representative ABC tune exercising fields, the K: tune-header
// terminator, notes with accidentals/octaves/lengths, a barline, a chord, a
// chord symbol, a decoration, and a comment — so highlights.scm has plenty to
// capture.
const SAMPLE = `X:1
T:Scale
M:4/4
L:1/8
K:C
"C" C D E F|G A B c|
[CEG]2 .a ^f'4| % a comment
`;

function fail(msg) {
  console.error(`verify:web FAIL — ${msg}`);
  process.exit(1);
}

async function main() {
  // Point the Emscripten runtime at the vendored runtime wasm so this works
  // offline and from any cwd (mirrors how the browser demo locates it).
  await Parser.init({
    locateFile(path, prefix) {
      if (path.endsWith(".wasm")) return RUNTIME_WASM;
      return prefix + path;
    },
  });

  const abc = await Language.load(readFileSync(GRAMMAR_WASM));
  const parser = new Parser();
  parser.setLanguage(abc);

  const tree = parser.parse(SAMPLE);
  const hasError = tree.rootNode.hasError;

  const query = new Query(abc, readFileSync(HIGHLIGHTS, "utf8"));
  const captures = query.captures(tree.rootNode);

  const captureCount = captures.length;
  // Distinct capture names actually exercised — handy signal in the log.
  const names = [...new Set(captures.map((c) => c.name))].sort();

  console.log(
    `verify:web sample-parse: root ERROR=${hasError ? 1 : 0}, highlight captures=${captureCount}, distinct capture names=${names.length} [${names.join(", ")}]`,
  );

  if (hasError) fail("parse tree root has an ERROR node");
  if (captureCount === 0) fail("highlight query produced 0 captures");

  console.log(
    `verify:web PASS — 0 ERROR nodes, ${captureCount} highlight captures (web-tree-sitter reuse proven headlessly)`,
  );
  process.exit(0);
}

main().catch((e) => fail(e?.stack || String(e)));
