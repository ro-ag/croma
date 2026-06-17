// demo.mjs — in-browser ABC syntax highlighting via web-tree-sitter + the
// committed `tree-sitter-abc.wasm` grammar. This is the headline "reuse proof":
// the SAME grammar that drives Zed / croma's GPUI app runs unchanged in a plain
// web page, with NO build step (the wasm is committed) and NO network (the
// web-tree-sitter runtime is vendored under web/vendor/).
//
// Pipeline: load runtime -> Language.load(grammar wasm) -> parse the sample ->
// run queries/highlights.scm -> paint each captured byte-range with a CSS class
// named for its capture (e.g. `tok-keyword`, `tok-string`). Theme lives in
// index.html so capture-name -> color stays editor-agnostic, exactly like a
// real tree-sitter consumer.

import {
  Parser,
  Language,
  Query,
} from "./vendor/web-tree-sitter.js";

// Paths are relative to web/ (this module's directory) so the demo is fully
// self-contained: `python3 -m http.server` from web/ serves everything, and it
// even works over file://. `web/tree-sitter-abc.wasm` and `web/highlights.scm`
// are committed copies of the canonical `../tree-sitter-abc.wasm` and
// `../queries/highlights.scm`; `npm run build:wasm` refreshes both. (We copy
// rather than reach into `..` because http.server refuses parent-dir paths.)
const GRAMMAR_WASM = "./tree-sitter-abc.wasm";
const RUNTIME_WASM = "./vendor/web-tree-sitter.wasm";
const HIGHLIGHTS_SCM = "./highlights.scm";

const SAMPLE = `X:1
T:Cooley's
C:Trad.
M:4/4
L:1/8
R:reel
K:Edor
|:D2|"Em"E2 BE B2 EB|~A2 FA DAFA|"D"D2 BD ADFD|.E2 .B,2 z2 :|
% a comment line
[V:1] "G"g2 fg edBd| {/e}dBAF DEFD|
`;

const $ = (id) => document.getElementById(id);

function setStatus(msg, kind = "info") {
  const el = $("status");
  el.textContent = msg;
  el.className = `status status-${kind}`;
}

// Render `text` into `container`, wrapping each highlight capture's byte-range
// in a <span class="tok-<capture>">. Captures can overlap / nest; tree-sitter
// returns them in source order, and a later capture for the same range wins in
// most themes, so we keep the LAST capture that covers each byte. We work on a
// per-byte class map for robustness against overlaps and multibyte chars.
function renderHighlighted(container, text, captures) {
  const bytes = new TextEncoder().encode(text);
  // class index per byte; -1 = no capture.
  const cls = new Array(bytes.length).fill(null);
  for (const cap of captures) {
    const klass = `tok-${cap.name.replace(/\./g, "-")}`;
    for (let i = cap.node.startIndex; i < cap.node.endIndex; i++) {
      cls[i] = klass; // last writer wins (source-ordered)
    }
  }

  const decoder = new TextDecoder();
  container.textContent = "";
  let i = 0;
  while (i < bytes.length) {
    const current = cls[i];
    let j = i + 1;
    while (j < bytes.length && cls[j] === current) j++;
    const chunk = decoder.decode(bytes.slice(i, j));
    if (current) {
      const span = document.createElement("span");
      span.className = current;
      span.textContent = chunk;
      container.appendChild(span);
    } else {
      container.appendChild(document.createTextNode(chunk));
    }
    i = j;
  }
}

async function main() {
  setStatus("Loading web-tree-sitter runtime…");
  await Parser.init({
    locateFile(path) {
      // The runtime asks for its own .wasm; serve the vendored copy.
      if (path.endsWith(".wasm")) return RUNTIME_WASM;
      return path;
    },
  });

  setStatus("Loading tree-sitter-abc.wasm grammar…");
  const abc = await Language.load(GRAMMAR_WASM);
  const parser = new Parser();
  parser.setLanguage(abc);

  setStatus("Fetching queries/highlights.scm…");
  const scm = await (await fetch(HIGHLIGHTS_SCM)).text();

  const editor = $("source");
  const out = $("output");

  function rerender() {
    const text = editor.value;
    const tree = parser.parse(text);
    const query = new Query(abc, scm);
    const captures = query.captures(tree.rootNode);
    renderHighlighted(out, text, captures);

    const distinct = new Set(captures.map((c) => c.name));
    const err = tree.rootNode.hasError;
    setStatus(
      `Parsed ${text.length} chars · ${captures.length} highlight captures · ${distinct.size} capture kinds · root ERROR: ${err ? "yes" : "no"}`,
      err ? "warn" : "ok",
    );
    query.delete();
    tree.delete();
  }

  editor.value = SAMPLE;
  editor.addEventListener("input", rerender);
  rerender();
}

main().catch((e) => {
  setStatus(`Failed: ${e?.message || e}`, "warn");
  // eslint-disable-next-line no-console
  console.error(e);
});
