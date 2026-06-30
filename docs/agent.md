# `croma agent` — help topics for AI agents

`croma agent` explains croma's **non-standard ABC notations** — the
`[I:croma-*]` / `%%croma-*` carriers — to an AI agent or LLM, so it can author
ABC annotations that persist through to MusicXML. It is a built-in, offline help
surface; nothing is fetched and no state is written.

```sh
croma agent                 # list every topic, grouped by category
croma agent <topic|alias>   # one notation: what it persists, syntax, example
```

- `croma agent` prints the **index**: each topic with a one-line summary.
- `croma agent <topic>` prints one page: what MusicXML fact the notation
  persists, its syntax, a **copy-paste ABC example**, and a `verify` command
  (`croma xml file.abc | grep <element>`).
- A topic is reachable by its short id (`xvoice-slur`), its full carrier name
  (`croma-xvoice-slur`), or a goal alias (`slur-across-voices`). An unknown
  topic exits non-zero and lists the available ids.

Start with `croma agent syntax` — the carrier convention itself (the two
vehicles, `key=value` fields, the `-hex=` rule for hostile characters).

## Relationship to `carriers.md`

[`carriers.md`](carriers.md) is the canonical, implementation-facing spec.
`croma agent` is its **agent-facing distillation**: the same carriers, framed as
tasks with runnable examples and no internal pointers. The topic database is a
JSON file embedded in the binary at build time; a unit test asserts every
carrier in `carriers.md` has a topic, so the two cannot drift.

## Typical agent loop

1. `croma agent` → find the topic for the MusicXML fact you need to preserve.
2. `croma agent <topic>` → copy the example, adapt it into the tune.
3. `croma xml tune.abc | grep <element>` → confirm the fact round-tripped.
