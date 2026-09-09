# FAQ

## Why is the parser strict?

Because strict recognition plus an explicit **diagnostic** is what makes croma
defensible and low-artifact. A silent best-effort guess is indistinguishable
from mimicking another tool's quirks; a warning is what makes recovery
transparent. The parser follows one three-tier rule:

1. **Default: reject** — input that doesn't match the ABC 2.1 grammar is not
   silently accepted.
2. **Recover *and warn*** — only for a clear intention spoiled by a trivial,
   mechanical slip (a stray space/comma); recovery is **never silent**.
3. **Otherwise: strict reject.**

Repair of genuinely loose source is the formatter's job, not the parser's:
`croma fmt --auto-fix` sanitises it into canonical spelling the strict parser
then reads cleanly ([[Formatter]]). Full policy:
[`AGENTS.md`](https://github.com/ro-ag/croma/blob/main/AGENTS.md).

## What's the status of ABC 2.2?

croma targets **ABC 2.1** as the stable spec; **ABC 2.2 is treated as a draft
compatibility mode.** Pass `--abc-2.2-draft` to `croma xml` / `check` / `fmt` /
`dump` to interpret a source against the ABC 2.2 draft. The default everywhere is
strict ABC 2.1.

## Why Rust?

- **Speed** — compiled, not interpreted: 7,081 ABC→MusicXML files/s and 43,247
  parse files/s over the 10k corpus ([[Benchmarks]]).
- **Embeddable** — `croma-core` is a **zero-dependency**, crates.io-publishable
  library, callable from any language via a native binary, not a script that
  needs a Python runtime.
- **Safety** — memory-safe with `unsafe` **forbidden** workspace-wide, a pinned
  toolchain, and `clippy -D warnings` + `cargo fmt` gates.

See [[abc2xml-Comparison]] for the head-to-head with the reference converter.

## Can I use croma commercially? What about attribution?

Yes. croma is licensed under **Apache-2.0**, so you may use it freely, including
in commercial products. In return, any redistribution or derivative work must
**retain the attribution** in
[`NOTICE`](https://github.com/ro-ag/croma/blob/main/NOTICE) (per section 4 of the
license) — i.e. commercial users must credit croma. The software is provided
**as-is, without warranty**, and the author carries **no liability** (sections
7–8). See [`LICENSE`](https://github.com/ro-ag/croma/blob/main/LICENSE).

## Does croma render sheet music / PDFs?

No. **PDF rendering and engraving layout are out of scope.** croma converts
between ABC and MusicXML, formats ABC, and provides editor tooling. To engrave,
feed croma's MusicXML output to a notation/engraving tool.

## How do I report a bug?

- **Bugs / feature requests:** open a
  [GitHub Issue](https://github.com/ro-ag/croma/issues). croma is a local,
  offline toolkit, so the most useful reports are crashes, hangs, or wrong/noisy
  output on a specific input — include the ABC (or MusicXML) that triggers it and
  the `croma` command you ran.
- **Security vulnerabilities:** do **not** open a public issue. Use GitHub's
  private vulnerability reporting — the repository's **Security** tab → **Report
  a vulnerability**. See
  [`SECURITY.md`](https://github.com/ro-ag/croma/blob/main/SECURITY.md).

More help with diagnostics and build issues: [[Troubleshooting]].
