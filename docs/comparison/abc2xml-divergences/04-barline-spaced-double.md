# 04 — Barline: spaced `| |` and line-break-split bars rendered `light-light`

**Affected:** the `barline` category touches 759 files; it is the **only**
divergence in 66 of them. The residual single-category cases are this pattern.
(Genuine barline bugs — repeat-start placement, volta number lists, liberal
*contiguous* combined barlines — were already fixed in earlier phases.)

## Cause

`abc2xml` renders a whitespace-separated `| |` (and a line-break-split
`…|\`-newline-`|…`) as a `light-light` thin-thin double barline. Croma treats
them as two ordinary single bar lines (no `<bar-style>`).

## ABC 2.1 basis

- §4.8 (lines 961–962): the "be liberal in recognizing bar lines" guidance
  applies to **contiguous** token sequences only — "bar lines may have any shape,
  using a *sequence* of `|` … e.g. `|[|` or `[|:::`." `||` (line 945) is the
  contiguous thin-thin double bar.
- §4.9 (lines 966–969): whitespace **is** significant around bar tokens —
  "`| [1` is legal, while `| 1` is not."

A space-separated `| |` is therefore two distinct thin bar lines, not the
contiguous `||` double bar.

## abc2xml vs Croma

- **`tune_002874`** (6 spaced `| |` boundaries, e.g. `…ef3/4g/2| |`): reference
  emits **6 `light-light`** + 2 `light-heavy` (`|]`); Croma emits only the 2
  `light-heavy`. Measure counts match (18 vs 18) — only the barline *type*
  differs.
- **`tune_002876`**: same pattern, 6 reference `light-light` vs 0.
- **`tune_004013`**: line-break-split form `…AGEF |\` then `| G2…` — reference 2
  `light-light` vs 0.

Croma still renders **true** contiguous doubles: in `tune_001312` a real `||`
yields one `light-light`; only the additional `|\`+`|` line-split is declined.

## Verdict

**ABC2XML_ARTIFACT.** Per §4.8/§4.9 non-contiguous bar tokens are not a double
bar; Croma's reading is spec-defensible. (A coalescing fix was attempted and
abandoned — it regressed the barline category by introducing the artifact into
many files; see `docs/progress/` phase notes.)
