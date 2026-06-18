# Releasing croma

How croma is versioned, how the binary-release CI works, and the exact steps to
cut a public release. This is a **runbook**: read it end-to-end before touching a
tag or a publish command.

> **Status (milestone `0.9.0`, "code readiness").** The release *mechanics* —
> crate metadata, `CHANGELOG.md`, and the binary-release CI in this document —
> are in place and dry-run-validated. **Nothing is published and no public
> release is cut.** `1.0.0` is reserved for the public launch. The full public
> cut below is intentionally **deferred** and must only be run on an explicit go.

## Overview: lockstep versioning

All four workspace crates share a single version via `[workspace.package].version`
in the root [`Cargo.toml`](../Cargo.toml):

- `croma-core` — the zero-dependency ABC ↔ MusicXML library.
- `croma-fmt` — the canonical ABC formatter.
- `croma-cli` — the `croma` CLI binary.
- `croma-lsp` — the stdio language server.

They are released **in lockstep**: one version number moves them all. The current
version is `0.9.0` ("code readiness"). `1.0.0` is reserved for the public launch
(first crates.io publish + first public GitHub Release).

## The asset-name contract

The binary CI uploads two binaries per platform. The **`croma-lsp-*`** column is
resolver-critical: the Zed extension's pure `asset_name` function in
[`editors/zed/src/lib.rs`](../editors/zed/src/lib.rs) computes exactly these names
and downloads the matching asset from the GitHub Release. A rename on either side
silently breaks the editor's auto-download path, so the names are pinned by unit
tests in that file. The **`croma-*`** column is the convenience CLI binary (same
naming scheme, not consumed by the resolver).

| Platform matrix entry | Rust target | `croma-lsp-*` asset (resolver-critical) | `croma-*` asset (CLI) |
| --- | --- | --- | --- |
| `macos-arm64` | `aarch64-apple-darwin` | `croma-lsp-macos-aarch64` | `croma-macos-aarch64` |
| `macos-x86_64` | `x86_64-apple-darwin` | `croma-lsp-macos-x86_64` | `croma-macos-x86_64` |
| `linux-x86_64` | `x86_64-unknown-linux-gnu` | `croma-lsp-linux-x86_64` | `croma-linux-x86_64` |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | `croma-lsp-linux-aarch64` | `croma-linux-aarch64` |
| `windows-x86_64` | `x86_64-pc-windows-msvc` | `croma-lsp-windows-x86_64.exe` | `croma-windows-x86_64.exe` |

Notes:

- The os labels are `macos` / `linux` / `windows`; the arch labels are `aarch64`
  / `x86_64`. **Only Windows gets the `.exe` suffix.**
- `linux-arm64` ships under the **`aarch64`** name (`croma-lsp-linux-aarch64`),
  not `linux-arm64` — the resolver maps `Architecture::Aarch64` to `aarch64`.
- The binaries are **uncompressed** bare executables. This matches the Zed
  resolver's `DownloadedFileType::Uncompressed`: it downloads the asset verbatim
  and marks it executable, with no archive to unpack.

## The release workflow

[`.github/workflows/release.yml`](../.github/workflows/release.yml) has two
trigger paths:

- **`push` of a `v*` tag** → the `build` matrix runs on all five platforms, then
  the `release` job (gated on `refs/tags/v*`) downloads every platform's artifacts
  and runs `gh release create` to attach all ten binaries to that tag's **GitHub
  Release**.
- **`workflow_dispatch`** → the `build` matrix runs and uploads the binaries as
  **Actions artifacts only**. The `release` job is skipped (no tag), so there is
  **no GitHub Release and no tag** — a fully reversible dry-run.

Runners and cross-compile:

- macOS arm64 / macOS x86_64 / linux-x86_64 / windows-x86_64 build **natively**
  on their respective GitHub-hosted runners.
- **`linux-arm64` is cross-compiled** from an x86_64 Ubuntu runner. croma's crates
  are pure Rust, so this needs nothing more than the `gcc-aarch64-linux-gnu`
  linker (set via `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER`); no `cross`
  and no Docker. This means the **dry-run covers all five platforms regardless of
  ARM-runner availability**.

Every build job ends with a **contract guard** in its "Stage named assets" step:
after copying the binaries into `dist/` under their contract names, it asserts
that the expected `croma-lsp-<os>-<arch>[.exe]` file exists and fails the job if
not — so a future rename that breaks the resolver fails CI loudly instead of
shipping a broken auto-download.

## Dry-run (no public effect)

Prove the whole binary CI without cutting anything:

```sh
# Kick off the matrix on main (no tag, no release).
gh workflow run release.yml --ref main

# Watch the run (or open it in the browser).
gh run watch
gh run view --log
```

When it is green, confirm the five per-platform artifacts exist and that the
`croma-lsp-*` names match the contract table above:

```sh
# List artifacts for the most recent release.yml run.
gh run view --json jobs,databaseId

# (Optional) download them locally to eyeball the names.
gh run download <run-id> -D /tmp/croma-dryrun && ls -R /tmp/croma-dryrun
```

You should see five artifacts — `croma-macos-arm64`, `croma-macos-x86_64`,
`croma-linux-x86_64`, `croma-linux-arm64`, `croma-windows-x86_64` — each
containing its `croma-*` and `croma-lsp-*` binary. This proves every platform
builds and every name matches the resolver, with **no tag and no release**.

## The full public cut (`1.0.0`, DEFERRED)

> **Irreversible. Do only on an explicit go.** crates.io allows *yank*, never
> *delete*; a pushed tag fires a public GitHub Release. Run this list only when
> the project is committed to a public `1.0.0` launch.

1. **Bump the version.** Set `[workspace.package].version` to `1.0.0` in the root
   `Cargo.toml`, and bump every internal path-dependency requirement to
   `version = "1.0.0"`. Move the `CHANGELOG.md` `[Unreleased]` items under a new
   `## [1.0.0] - <date>` heading. Land it via
   `uv run tools/land.py <branch> -y`.
2. **Tag the merge commit.** `git tag v1.0.0 && git push origin v1.0.0`. The
   pushed tag fires `release.yml`, which builds the matrix and creates the GitHub
   Release with all binaries attached.
3. **Publish the crates, in dependency order.** Publish
   `croma-core` → `croma-fmt` → `croma-cli` → `croma-lsp`, **waiting for each
   crate to appear on the crates.io index before publishing the next dependent**
   (a dependent will not resolve until its dependency is live). Requires
   `cargo login` with a crates.io token.

   ```sh
   cargo publish -p croma-core
   # wait for croma-core to land on the index, then:
   cargo publish -p croma-fmt
   cargo publish -p croma-cli
   cargo publish -p croma-lsp
   ```

   This step is **irreversible** — crates.io allows yank but never delete.
4. **Verify auto-download end-to-end.** Install/enable the Zed extension, open an
   `.abc` file on a machine with **no `croma-lsp` on `PATH`**, and confirm Zed
   fetches the published `croma-lsp` binary from the GitHub Release and attaches
   the server.

## Pre-cut checklist

Before any public cut, all of the following must be green:

- **All proven gates:**
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --all --check`
  - zero-dependency guard: `cargo tree -p croma-core --edges normal | wc -l` == `1`
  - LSP legs (diagnostics / formatting / totality / tokens / latency over 10k)
  - fmt idempotent + lossless over the 10k-file corpus
  - MusicXML reader self-loop + foreign parity
  - `tree-sitter-abc` grammar corpus parse
- **Publish set is publishable:** `cargo publish --dry-run` clean for
  `croma-core`, `croma-fmt`, `croma-cli`, and `croma-lsp`.
- **The dry-run workflow is green** with correctly-named assets (see
  [Dry-run](#dry-run-no-public-effect)) — every `croma-lsp-*` name matches the
  [asset-name contract](#the-asset-name-contract).
