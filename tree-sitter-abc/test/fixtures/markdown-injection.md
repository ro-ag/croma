# Markdown ` ```abc ` injection fixture

This file exercises `queries/markdown-injection.scm`: a Markdown consumer that
loads that injection query should parse the fenced block below with the
`tree-sitter-abc` grammar and highlight it as ABC.

A complete tune in a fenced `abc` block:

```abc
X:1
T:Cooley's
M:4/4
L:1/8
R:reel
K:Edor
|:D2|"Em"E2 BE B2 EB|~A2 FA DAFA|"D"D2 BD ADFD|.E2 .B,2 z2 :|
[V:1] "G"g2 fg edBd| {/e}dBAF DEFD| % an inline comment
```

A second `abc` block, to confirm the rule matches every fenced block (not just
the first):

```abc
X:2
T:Scale
K:C
CDEF GABc|
```

A non-`abc` fence (here `rust`) must NOT be injected with the ABC grammar — it
is left to whatever the host wires for that label:

```rust
fn main() {
    println!("not abc");
}
```
