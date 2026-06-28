# CLI usage

The `croma` binary is a thin wrapper over [[Library-Usage|the core library]].
All examples below are runnable against the repo (the sample file is
[`examples/basic.abc`](https://github.com/ro-ag/croma/blob/main/examples/basic.abc)).

```text
$ croma --help
ABC notation toolkit

Usage: croma <COMMAND>

Commands:
  xml           Export an ABC file to MusicXML
  check         Parse and validate an ABC file, reporting diagnostics only
  dump          Dump intermediate representations for debugging
  fmt           Format an ABC file (canonical formatting, in the style of rustfmt/gofmt)
  read          Read a MusicXML file back into a Score and project it (experimental)
  musicxml2abc  Convert a MusicXML file to ABC (read MusicXML -> Score -> write ABC)
  help          Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

`read` and `musicxml2abc` are present only when the CLI is built with the
(default) `musicxml-reader` feature; a `--no-default-features` build omits them.

## `croma xml` — ABC → MusicXML

Export an ABC file to MusicXML 4.0. Writes to stdout unless `-o/--output` is
given.

```sh
croma xml tune.abc > tune.musicxml      # stdout redirect
croma xml tune.abc -o tune.musicxml     # explicit output path
```

```text
$ croma xml examples/basic.abc
<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <work>
    <work-title>Scale</work-title>
  </work>
  <part-list>
    <score-part id="P1">
      <part-name>Scale</part-name>
    </score-part>
  </part-list>
  ...
```

Flags: `-o/--output <PATH>`, the [parse-mode flags](#parse-modes-shared-flags),
`--diagnostics <text|json>`, `--warnings-as-errors`.

## `croma check` — validate, diagnostics only

Parse and validate an ABC file; report diagnostics without producing output.
Exit code is non-zero when there are errors (or warnings, with
`--warnings-as-errors`).

```sh
croma check tune.abc                      # human-readable
croma check --diagnostics=json tune.abc   # machine-readable
```

A clean file prints nothing and exits 0. A real diagnostic, text form:

```text
$ croma check nokey.abc
nokey.abc:3:1-3:1: error[abc.file.missing_k]: ABC source is missing a K: field
  byte span 13..13
  |
  | ^
```

The same diagnostic as JSON (`--diagnostics=json`) carries the code, message,
severity, 1-based `line_span`, byte `span`, `path`, and a `snippet`:

```json
[
  {
    "code": "abc.file.missing_k",
    "line_span": {
      "end": { "column": 1, "line": 3 },
      "start": { "column": 1, "line": 3 }
    },
    "message": "ABC source is missing a K: field",
    "path": "nokey.abc",
    "severity": "error",
    "snippet": { "line": "", "marker": "^" },
    "span": { "end": 13, "start": 13 }
  }
]
```

See [[Troubleshooting]] for how to read codes and spans.

## `croma fmt` — canonical formatting

Format an ABC file in the style of `rustfmt` / `gofmt`. Prints to stdout by
default.

```sh
croma fmt tune.abc            # print canonical formatting to stdout
croma fmt --check tune.abc    # exit non-zero if FILE is not already canonical
croma fmt --write tune.abc    # rewrite FILE in place
croma fmt --auto-fix tune.abc # also apply safe, pitch-preserving repairs to loose source
```

`--check` and `--write` are mutually exclusive. Plain `fmt` is **lossless**
(only inter-token whitespace, blank-line runs, and the final newline are
normalized); `--auto-fix` additionally applies a gated catalogue of repairs,
each reverted if it would change the score. Full reference: [[Formatter]].

`fmt`-specific flags: `--check`, `-w/--write`, `--auto-fix`, plus the shared
parse-mode / diagnostics / `--warnings-as-errors` flags.

## `croma read` / `croma musicxml2abc` — MusicXML → ABC

The reverse reader. `read` projects the reconstructed `Score` in one of three
formats; `musicxml2abc` is a discoverable alias for `read --format abc`.

```sh
croma read score.musicxml --format abc    # MusicXML -> ABC
croma read score.musicxml --format xml    # re-emit MusicXML (the writer's inverse)
croma read score.musicxml --format dump   # pretty-print the reconstructed Score
croma musicxml2abc score.musicxml         # = read --format abc
```

`--format` values: `xml` (default — re-emit MusicXML), `abc` (write ABC),
`dump` (debug-print the `Score`). Both subcommands accept `-o/--output <PATH>`.
Round-tripping the scale above back through the reader:

```text
$ croma xml examples/basic.abc -o basic.musicxml
$ croma musicxml2abc basic.musicxml
X:
T:Scale
M:4/4
L:1/8
K:C
V:P1
C D E F | G A B c
```

Reader scope, foreign-dialect support, and the adjudicated residual:
[[MusicXML-Reader]].

## `croma dump` — intermediate representations

A debugging aid that prints an intermediate representation rather than a final
artifact.

```sh
croma dump tokens tune.abc    # the music-token stream
croma dump tree   tune.abc    # the parsed syntax tree
croma dump score  tune.abc    # the lowered Score model
croma dump abc    tune.abc    # re-emitted ABC from the model
```

`<KIND>` is one of `tokens`, `tree`, `score`, `abc`. Accepts the shared
parse-mode / diagnostics / `--warnings-as-errors` flags.

## Parse modes (shared flags)

`xml`, `check`, `fmt`, and `dump` share a parser-mode group and diagnostics
flags:

| Flag | Effect |
| --- | --- |
| `--strict` | Strict ABC 2.1 parsing (**default**) |
| `--loose` | Loose parsing mode |
| `--recover` | Recover parsing mode |
| `--abc-2.2-draft` | Interpret the source against the ABC 2.2 draft spec |
| `--diagnostics <text\|json>` | Diagnostics output format (default `text`) |
| `--warnings-as-errors` | Exit non-zero if any warning is present |

The parser defaults to **strict**; loose-source repair is the formatter's job
(`croma fmt --auto-fix`), not the parser's. See the parser recovery policy in
[`AGENTS.md`](https://github.com/ro-ag/croma/blob/main/AGENTS.md) and the
[[FAQ#why-is-the-parser-strict]].
