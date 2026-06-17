# croma ABC — Zed extension

Native [ABC music notation](https://abcnotation.com/) support for the
[Zed editor](https://zed.dev/): syntax highlighting, code folding, and bracket
matching via the in-repo [`tree-sitter-abc`](../../tree-sitter-abc) grammar, plus
diagnostics / formatting / hover / completion / symbols by launching the
[`croma-lsp`](../../crates/croma-lsp) language server.

> Zed = GPUI, the same engine as croma's GPUI app. This extension is also a
> real-engine smoke test of `croma-lsp`.

## What you get

- **Syntax highlighting** — pitches, accidentals, lengths, rests, tuplets,
  barlines/repeats, slurs/ties, chord symbols, annotations, decorations, header
  fields, inline fields, comments, and `%%` stylesheet directives.
- **Folding** — fold a whole tune, just its body, or just its header.
- **Bracket matching** — chord `[ … ]` and grace `{ … }` groups, quoted strings.
- **Language server** — everything `croma-lsp` provides over stdio (diagnostics,
  formatting, semantic tokens, document symbols, folding ranges, hover,
  completion, code actions).

Files with the `.abc` suffix are recognized as the `ABC` language.

## Prerequisites: the `croma-lsp` binary

The extension resolves the language server with a **download-or-PATH** strategy:

1. **`croma-lsp` on your `PATH`** (works today):

   ```sh
   # from a croma checkout
   cargo install --path crates/croma-lsp
   ```

   This installs `croma-lsp` into `~/.cargo/bin`; make sure that is on your
   `PATH`. Once croma-lsp is published you will be able to
   `cargo install croma-lsp`.

2. **Automatic GitHub-release download** — when croma cuts binary releases, the
   extension downloads the right `croma-lsp` for your platform automatically. No
   manual install needed. (Not yet functional — pending the release epic.)

3. Otherwise the extension shows a clear error pointing you back at step 1.

## Dev-installing the extension in Zed

This extension is not yet published to the Zed extension registry, so install it
as a **dev extension**:

1. Build / install `croma-lsp` so it is on your `PATH` (see above).
2. In Zed, open the command palette and run **`zed: install dev extension`**.
3. Select **this directory** (`editors/zed/`).

Zed compiles the extension to WebAssembly, builds the grammar, and registers the
`ABC` language. Open any `.abc` file to verify highlighting; the status bar
should show the `croma-lsp` language server attaching.

> The grammar is wired by repository + monorepo `path` (`tree-sitter-abc`),
> which requires a reasonably recent Zed that supports the `path` key in
> `[grammars.*]`.

## Building / testing the extension crate directly

This crate (`zed_croma_abc`) is **intentionally excluded** from the croma cargo
workspace (it targets wasm and pulls in `zed_extension_api`). Build and test it
from this directory:

```sh
# compile to the wasm target Zed loads
cargo build --release --target wasm32-wasip1

# run the host-side unit tests (the pure release-asset-name resolver)
cargo test

# lint / format
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Layout

```
editors/zed/
├── extension.toml          # extension manifest: grammar pin + croma-lsp server
├── Cargo.toml              # wasm extension crate (excluded from the workspace)
├── src/lib.rs              # Extension impl + download-or-PATH resolver
└── languages/abc/
    ├── config.toml         # name/suffix/comments/brackets/autoclose
    ├── highlights.scm      # Zed copies of the canonical grammar queries,
    ├── folds.scm           #   kept in sync with tree-sitter-abc/queries/*
    └── brackets.scm
```
