# croma-lsp

`croma-lsp` is the language server for ABC notation, part of
[Croma](https://github.com/ro-ag/croma). It installs the `croma-lsp` binary, a
stdio [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
implementation that is a thin adapter over the `croma-core` parser and the
`croma-fmt` formatter.

Supported capabilities include diagnostics, formatting, semantic tokens, document
symbols, folding ranges, hover, completion, and code actions. The server shares
its parse and surface model with the rest of the toolkit, so editor feedback
matches the `croma` CLI exactly.

```sh
cargo install croma-lsp
```

The server speaks LSP over stdio and is consumed by the
[Zed extension](https://github.com/ro-ag/croma/tree/main/editors/zed) and any
LSP-capable editor. See the
[LSP documentation](https://github.com/ro-ag/croma/blob/main/docs/lsp.md) for
capability details and editor wiring.
