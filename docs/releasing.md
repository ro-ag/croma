# Releasing croma

How croma is versioned, how the release workflows work, and the exact steps to
cut a release. This is a **runbook**: read it end-to-end before touching a tag.

Releases publish **only** from GitHub Actions when a `v*` tag is pushed. Never run
`cargo publish` or `gh release create` by hand.

## Overview: lockstep versioning

All four workspace crates share a single version via `[workspace.package].version`
in the root [`Cargo.toml`](../Cargo.toml):

- `croma-core` — the zero-dependency ABC ↔ MusicXML library.
- `croma-fmt` — the canonical ABC formatter.
- `croma-cli` — the `croma` CLI binary.
- `croma-lsp` — the stdio language server.

They are released **in lockstep**: one version number moves them all, and every
internal path dependency pins that same version. The released versions and their
changes are in [`CHANGELOG.md`](../CHANGELOG.md).

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

- **`push` of a `v*` tag** → the Linux `verify` job (fmt, clippy, workspace
  tests, reader feature) runs first; only if it passes does the `build` matrix
  run on all five platforms, then the `release` job (gated on `refs/tags/v*`)
  downloads every platform's artifacts and runs `gh release create` to attach all
  ten binaries to that tag's **GitHub Release**.
- **`workflow_dispatch`** → `verify` and the `build` matrix run and upload the
  binaries as **Actions artifacts only**. The `release` job is skipped (no tag),
  so there is **no GitHub Release and no tag** — a fully reversible dry-run.

[`.github/workflows/publish.yml`](../.github/workflows/publish.yml) fires on the
same `v*` tag. It runs the same `verify` job and then publishes the crates in
dependency order (`croma-core` → `croma-fmt` → `croma-cli` → `croma-lsp`),
retrying while the crates.io index propagates and treating an already-published
version as success. Both jobs only run on a `v*` tag: a manual dispatch from a
branch publishes nothing.

Runners and cross-compile:

- **macOS arm64**, **linux-x86_64**, and **windows-x86_64** build **natively** on
  their GitHub-hosted runners (`macos-14`, `ubuntu-22.04`, `windows-latest`).
- **`macos-x86_64` is cross-compiled** on the arm64 `macos-14` runner. Apple's
  toolchain targets x86_64 from an arm host with the universal SDK, and croma's
  crates are pure Rust, so no extra linker is needed. This deliberately avoids the
  scarce Intel `macos-13` runner pool, which can queue indefinitely (it stalled a
  dry-run for 25+ min) and is being deprecated.
- **`linux-arm64` is cross-compiled** from the x86_64 `ubuntu-22.04` runner.
  Pure-Rust crates need only the `gcc-aarch64-linux-gnu` linker (set via
  `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER`); no `cross`, no Docker.
- Net: the **dry-run covers all five platforms reliably, independent of Intel-mac
  and ARM-linux hosted-runner availability** — every target either builds natively
  on a plentiful runner or is cross-compiled on one.

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

## Cutting a release

> **Irreversible once the tag is pushed.** crates.io allows *yank*, never
> *delete*, and the tag fires a public GitHub Release. Push a tag only on an
> explicit go for that release.

1. **Branch and bump.** On a fresh branch, set `[workspace.package].version` in
   the root `Cargo.toml` and every internal path-dependency `version = "…"` to the
   new version. Move the `CHANGELOG.md` `[Unreleased]` items under a new
   `## [X.Y.Z] - <date>` heading and add its compare link at the bottom. Update
   any version shown in `README.md` and the crate READMEs.
2. **Run the full gate** ([below](#release-gate)), including the semver check
   against the previous tag.
3. **Land it** via PR and squash merge with a `chore(release): X.Y.Z` title
   (`uv run tools/land.py <branch> -y`).
4. **Tag the merge commit on `main` and push the tag** (only on the explicit go):
   `git tag vX.Y.Z <merge-commit> && git push origin vX.Y.Z`.
5. **Watch both workflows** (`gh run list --workflow publish.yml`,
   `gh run list --workflow release.yml`) until green, then confirm crates.io shows
   the new version for all four crates and the GitHub Release has ten binaries.
6. **For a Zed-visible change**, open an `.abc` file on a machine with no
   `croma-lsp` on `PATH` and confirm Zed downloads the new server binary.

## Release gate

Everything here must be green on the commit you will tag:

- **Workspace checks** (with the pinned toolchain from `rust-toolchain.toml`
  first on `PATH`):
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `cargo test -p croma-core --features musicxml-reader`
  - `cargo clippy -p croma-core --all-targets --features musicxml-reader -- -D warnings`
  - zero-dependency guard: `cargo tree -p croma-core --edges normal | wc -l` == `1`
- **Corpus gates** in [croma-test](https://github.com/ro-ag/croma-test)
  (`CROMA_DIR=$PWD ./croma-test/bootstrap.sh --with-reference`): fmt 10k lossless,
  reader round-trip, LSP totality and fidelity, grammar coverage, and the abc2xml
  whitelist. Compare each against its recorded baseline.
- **Semver check** of the library crates against the previous release tag
  (needs `cargo install cargo-semver-checks`):

  ```sh
  cargo semver-checks --baseline-rev v<previous-version> --all-features \
    -p croma-core -p croma-fmt -p croma-lsp
  ```

  A reported break needs a decision before tagging: bump the major version, or
  confirm it falls under the documented 1.x semver scope and note it in the
  changelog. `croma-cli` is binary-only and is not checked.
- **Publish set is publishable:** `cargo publish --dry-run -p <crate>` is clean
  for all four crates.
- **The dry-run workflow is green** with correctly-named assets (see
  [Dry-run](#dry-run-no-public-effect)).
