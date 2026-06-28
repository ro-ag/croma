# Installation

croma ships as four crates from one workspace, all versioned in lockstep at
**1.0.1** under **Apache-2.0**. Pick the route that fits your use:

- **Just want the `croma` command?** → [crates.io](#from-cratesio) or a
  [prebuilt binary](#prebuilt-release-binaries).
- **Embedding ABC ↔ MusicXML in a Rust app?** → `cargo add croma-core`, then
  see [[Library-Usage]].
- **Wiring up an editor?** → install `croma-lsp` and see [[Editors-and-Zed]].

## From crates.io

```sh
cargo install croma-cli      # the `croma` CLI
cargo install croma-lsp      # the stdio language server
cargo add croma-core         # the library (ABC <-> MusicXML, zero-dependency)
```

`cargo install croma-cli` builds the binary named **`croma`** (not
`croma-cli`). The CLI's default build includes the MusicXML reader; for a
reader-less, zero-dependency CLI see [below](#reader-less-zero-dependency-cli).

## Prebuilt release binaries

Each [GitHub Release](https://github.com/ro-ag/croma/releases) attaches **two
uncompressed binaries per platform** — `croma-*` (the CLI) and `croma-lsp-*`
(the language server). Download the one matching your platform, mark it
executable, and put it on your `PATH`.

| Platform | Rust target | CLI asset | Language-server asset |
| --- | --- | --- | --- |
| macOS arm64 | `aarch64-apple-darwin` | `croma-macos-aarch64` | `croma-lsp-macos-aarch64` |
| macOS x86_64 | `x86_64-apple-darwin` | `croma-macos-x86_64` | `croma-lsp-macos-x86_64` |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `croma-linux-x86_64` | `croma-lsp-linux-x86_64` |
| Linux arm64 | `aarch64-unknown-linux-gnu` | `croma-linux-aarch64` | `croma-lsp-linux-aarch64` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `croma-windows-x86_64.exe` | `croma-lsp-windows-x86_64.exe` |

Notes:

- OS labels are `macos` / `linux` / `windows`; arch labels are `aarch64` /
  `x86_64`. **Only Windows carries the `.exe` suffix.** Linux arm64 ships under
  the `aarch64` name, not `linux-arm64`.
- The assets are **bare, uncompressed executables** — there is no archive to
  unpack.
- The `croma-lsp-*` names are a pinned contract: the Zed extension's resolver
  computes exactly these to auto-download the server. Full detail:
  [`docs/releasing.md`](https://github.com/ro-ag/croma/blob/main/docs/releasing.md).

## From source

croma pins **Rust 1.96.0** via
[`rust-toolchain.toml`](https://github.com/ro-ag/croma/blob/main/rust-toolchain.toml),
so a plain `cargo` / `rustc` selects the right toolchain on any host (edition
2024).

```sh
git clone https://github.com/ro-ag/croma
cd croma
cargo build --release        # target/release/croma (CLI) + croma-lsp
```

The default build's `default-members` are `croma-core`, `croma-cli`, and
`croma-lsp`, so `cargo build` produces both the `croma` CLI and the `croma-lsp`
server.

### Reader-less, zero-dependency CLI

The MusicXML reader's only dependency (`roxmltree`) is opt-in. Build a CLI
without it — and without `croma read` / `croma musicxml2abc` — with:

```sh
cargo build -p croma-cli --no-default-features
```

This is the same zero-dependency surface the `croma-core` **library** ships by
default; see [[Library-Usage]] for the library side.

## Verify the install

```sh
croma --version      # croma 1.0.1
croma --help
```

Next: [[CLI-Usage]].
