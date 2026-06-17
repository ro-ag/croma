# tree-sitter-abc — web / WASM demo

The headline **reuse proof**: the same `tree-sitter-abc` grammar that drives the
Zed extension and croma's GPUI app, compiled to WebAssembly and run unchanged in
a browser via [`web-tree-sitter`](https://www.npmjs.com/package/web-tree-sitter)
to highlight ABC live.

## View the demo

It is fully self-contained — no build step (the grammar wasm is committed) and
no network (the `web-tree-sitter` runtime is vendored under `vendor/`). Browsers
block ES-module + `fetch` over `file://`, so serve the directory:

```sh
cd tree-sitter-abc/web
python3 -m http.server 8000
# open http://localhost:8000/
```

Edit the ABC in the left pane; the right pane re-highlights live via
`highlights.scm`. The status line reports the parse: char count, number of
highlight captures, distinct capture kinds, and whether the root has an ERROR.

## Headless gate (no browser)

`verify.mjs` is the automatable proof that the web reuse works. It loads
`web-tree-sitter` + the committed grammar wasm, parses a sample, runs
`queries/highlights.scm`, and **asserts the parse tree has no root ERROR and the
query yields > 0 captures** (exit non-zero on failure):

```sh
cd tree-sitter-abc
npm install          # pulls web-tree-sitter@0.26.9 (a devDependency) into node_modules
npm run verify:web   # -> "verify:web PASS — 0 ERROR nodes, N highlight captures"
```

## Rebuild the grammar wasm

```sh
cd tree-sitter-abc
npm run build:wasm   # tree-sitter build --wasm -> tree-sitter-abc.wasm (+ refreshes the web/ copies)
```

`tree-sitter build --wasm` compiles the grammar's C source to WebAssembly using
**wasi-sdk**, which the tree-sitter CLI (≥ 0.26) auto-downloads and caches under
`~/.cache/tree-sitter/wasi-sdk` on first run. **No Docker, Emscripten, or Podman
is required** with a recent CLI — despite older docs that mention them. The
output is a ~26 KB `tree-sitter-abc.wasm`, committed so the demo (and any
`web-tree-sitter` consumer) needs no build step.

## Files

| Path | What it is | Committed? |
|---|---|---|
| `index.html` | demo page + the capture-name → color theme (editor-agnostic) | yes |
| `demo.mjs` | loads runtime + grammar, parses, runs `highlights.scm`, paints `tok-<capture>` spans | yes |
| `verify.mjs` | headless assertion gate (`npm run verify:web`) | yes |
| `tree-sitter-abc.wasm` | committed copy of `../tree-sitter-abc.wasm` (the grammar) | yes |
| `highlights.scm` | committed copy of `../queries/highlights.scm` | yes |
| `vendor/web-tree-sitter.js` | vendored `web-tree-sitter` **0.26.9** ES module (version-matched to the CLI) | yes |
| `vendor/web-tree-sitter.wasm` | vendored `web-tree-sitter` runtime wasm | yes |

The `web/` copies of `tree-sitter-abc.wasm` and `highlights.scm` exist because
`python3 -m http.server` (and `file://`) refuse parent-directory (`..`) paths, so
the demo must reach all its assets from inside `web/`. `npm run build:wasm`
refreshes both from their canonical sources at the package root.

`vendor/` is pinned to **`web-tree-sitter@0.26.9`**, the exact version of the
`tree-sitter` CLI that produced `tree-sitter-abc.wasm`; the grammar-wasm ABI and
the runtime must match. To re-vendor after bumping the CLI:

```sh
cd tree-sitter-abc
npm install
cp node_modules/web-tree-sitter/web-tree-sitter.js  web/vendor/web-tree-sitter.js
cp node_modules/web-tree-sitter/web-tree-sitter.wasm web/vendor/web-tree-sitter.wasm
```
