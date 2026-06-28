# How croma compares to abc2xml

[`abc2xml`](https://wim.vree.org/svgParse/abc2xml.html) (by Willem Vree) is the
long-standing reference ABC → MusicXML converter, and it is croma's correctness
**baseline**: croma is validated against it over the full 10k corpus. Where
abc2xml is spec-correct, croma matches it (**9,390 / 9,390** structural matches);
croma diverges **only** where abc2xml departs from the ABC 2.1 spec, and every
such case is adjudicated and documented.

croma is a from-scratch, library-first reimplementation that improves on the
reference in several ways:

| | abc2xml | croma |
| --- | --- | --- |
| **Direction** | ABC → MusicXML (the reverse is a separate script, `xml2abc`) | ABC ↔ MusicXML in one library |
| **Form** | a Python script (needs a Python runtime) | a Rust library + native binaries — zero-dependency, embeddable, crates.io-publishable, callable from any language |
| **Speed** | interpreted Python | compiled Rust — **7,081** ABC→MusicXML files/s and **43,247** parse files/s over the 10k corpus |
| **Malformed input** | permissive — silent best-effort heuristics | strict ABC 2.1 — structured **diagnostics** (codes + spans); recovers only when the intent is unambiguous, and always warns |
| **Output artifacts** | inserts spurious elements as heuristic side effects — e.g. empty leading/section measures, phantom measures | spec-faithful, **minimal** MusicXML — declines those artifacts; most croma↔abc2xml divergences are exactly such an artifact croma omits |
| **Beyond conversion** | converter only | also a **formatter** (idempotent + lossless), a **language server**, a reusable **tree-sitter grammar**, and a **Zed extension** |
| **Validation** | — | corpus-proven over 10k with a documented gate matrix + a reproducible benchmark baseline |
| **Safety** | — | memory-safe Rust (`unsafe` forbidden), pinned toolchain, clippy + fmt gates |

In short: abc2xml is a mature one-way converter script; croma is a fast,
embeddable, **bidirectional** ABC toolkit that holds **stricter to the spec**
and emits **cleaner, lower-artifact** MusicXML, with editor-grade diagnostics —
designed to be linked into applications and proven at scale.

## Why divergences are a feature, not a regression

Most croma ↔ abc2xml divergences are an artifact that abc2xml inserts and croma
declines (an empty leading measure, a phantom measure). croma treats abc2xml as
the *baseline* precisely so it can earn its correctness claims: matching it on
the 9,390 spec-correct files, and **adjudicating** every remaining file as a
documented, spec-justified difference rather than a silent guess. The comparator,
the whitelist/dropped baseline, the ABC spec knowledge base, and the
divergence-triage process all live in the companion **croma-test** repo (see
[[How-its-Proven]]).

croma also stands on abc2xml's shoulders: using it as the parity baseline is how
croma's correctness is measured in the first place.
