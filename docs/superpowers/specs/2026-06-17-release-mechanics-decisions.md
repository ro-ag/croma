# Release mechanics — decisions (Epic C)

**Date:** 2026-06-17 · **Phase:** `phase-67-release-*` · **Base:** main @ `6ba4989`

croma's four capabilities are corpus-proven (forward writer 9390/0, `croma fmt`
10000/0, MusicXML→ABC reader 9935/9935 + 98.50%, `croma-lsp` legs A–E), plus a
`tree-sitter-abc` grammar and a Zed extension. Code is "feature gold" but **not a
cut release**: all crates are `0.1.0`, there are no git tags, no CHANGELOG, no
crates.io publish, no binary releases. This epic does the release mechanics and
the per-platform binary-release CI that lights up the Zed extension auto-download.

This doc records the decisions. **User scope call (2026-06-17): this is the
"code readiness" milestone — cut at `0.9.0`, NOT `1.0.0`.** Everything is prepared
and dry-run for crates.io, but **no `cargo publish`, no public tag, and no public
GitHub release happen** — those are deferred to the `1.0.0` public launch. C1 + C2
are built and landed (via `land.py`) as code-readiness; C3 (the irreversible cut)
is deferred wholesale.

---

## 1. Version scheme — **lockstep `0.9.0`** (user-decided 2026-06-17)

All four crates currently hardcode `version = "0.1.0"` (not workspace-inherited).

**Decision: lockstep — all crates share one version — and cut at `0.9.0`.**

- **Why `0.9.0` (user call):** this is a *code-readiness* milestone, not the public
  launch. `0.x` keeps SemVer freedom to break the API while the repo is still
  private, and `0.9` reads as "release-candidate-ish, one step from public".
  **`1.0.0` is reserved for the public launch** (when the repo goes public + crates
  are actually published). So the jump is `0.1.0 → 0.9.0` now, `→ 1.0.0` later.
- **Why lockstep:** the four crates are one product released together; a single
  version is the simplest mental model and the future release tag (`vX.Y.Z`) maps
  to exactly one number. Mechanism: add `version = "0.9.0"` to
  `[workspace.package]` and switch each crate to `version.workspace = true`, so
  lockstep is enforced by construction (one edit bumps all four).
- Path-dep requirements (see §2) get the matching `version = "0.9.0"`
  (caret `^0.9.0` = `>=0.9.0, <0.10.0`, valid within the 0.9.x lockstep).

---

## 2. Publish set + order — **all four, dependency order**

**Set:** `croma-core`, `croma-fmt`, `croma-cli`, `croma-lsp` — **prepared + dry-run
for all four** (the actual publish is deferred to 1.0, per the scope call above).

- `croma-cli` (bin `croma`) and `croma-lsp` (bin `croma-lsp`) are binary crates;
  publishing them (later) enables `cargo install croma-cli` / `cargo install
  croma-lsp` — the latter is exactly the fallback the Zed resolver's error message
  already documents. So all four get publish-ready metadata now.

**Order (dependency order, for the eventual publish):** `croma-core` → `croma-fmt`
→ `croma-cli` → `croma-lsp`. `croma-cli` and `croma-lsp` both depend on core+fmt
and are independent of each other; the linear order above is valid.

**Blocking metadata work (cargo strips `path` on publish — path deps MUST carry a
version or the publish fails):**

- `croma-fmt` dep on `croma-core` → add `version = "0.9.0"`.
- `croma-cli` deps on `croma-core`, `croma-fmt` → add `version = "0.9.0"` to each.
- `croma-lsp` deps on `croma-core`, `croma-fmt` → add `version = "0.9.0"` to each.

**Dry-run note:** a dependent crate's `--dry-run` *verify build* needs its siblings
on a registry (they are not published), so verify with `cargo publish --dry-run -p
<crate>` and, for the dependents, fall back to `--no-verify` to confirm
metadata/packaging (cargo still errors on a path-only dep or missing metadata under
`--no-verify`). Record exactly which mode each crate used.

**Metadata completeness (crates.io):** `license` + `repository` are inherited from
`[workspace.package]` for all four ✓. `croma-core` already has `description`,
`readme`, `keywords`, `categories`. **`croma-fmt`/`croma-cli`/`croma-lsp` are
missing `readme`, `keywords`, `categories`** → add them (per-crate `README.md` +
relevant keyword/category sets). `authors` is optional on modern crates.io and is
omitted (consistent with `croma-core` today).

> Naming note (not a blocker): no crate is named bare `croma`; `cargo install
> croma-cli` installs the `croma` binary. Reserving the bare `croma` crate name is
> out of scope for this epic.

---

## 3. Binary-release CI + asset scheme — **tag `v*`, native matrix runners**

A GitHub Actions workflow triggered on tag `v*` matrix-builds the `croma` and
`croma-lsp` binaries for five targets and attaches them to the GitHub release.

**Native runners** (recommended over `cross`) — GitHub-hosted ARM Linux runners
are GA, so every target builds natively with no cross toolchain:

| target        | runner            | rust target triple           |
|---------------|-------------------|------------------------------|
| macos-arm64   | `macos-14`        | `aarch64-apple-darwin`       |
| macos-x86_64  | `macos-13`        | `x86_64-apple-darwin`        |
| linux-x86_64  | `ubuntu-22.04`    | `x86_64-unknown-linux-gnu`   |
| linux-arm64   | `ubuntu-22.04-arm`| `aarch64-unknown-linux-gnu`  |
| windows-x86_64| `windows-latest`  | `x86_64-pc-windows-msvc`     |

**Asset-name scheme — THE CONTRACT with the Zed resolver** (`editors/zed/src/lib.rs`
`asset_name`). Bare, **uncompressed** binaries (resolver uses
`DownloadedFileType::Uncompressed`). The resolver maps `Os::Mac→macos`,
`Os::Linux→linux`, `Os::Windows→windows` and `Aarch64→aarch64`, `X8664→x86_64`,
so **linux-arm64 is named `linux-aarch64`, NOT `linux-arm64`**:

| target        | `croma-lsp` asset (resolver-critical) | `croma` asset (convenience) |
|---------------|---------------------------------------|-----------------------------|
| macos-arm64   | `croma-lsp-macos-aarch64`             | `croma-macos-aarch64`       |
| macos-x86_64  | `croma-lsp-macos-x86_64`              | `croma-macos-x86_64`        |
| linux-x86_64  | `croma-lsp-linux-x86_64`              | `croma-linux-x86_64`        |
| linux-arm64   | `croma-lsp-linux-aarch64`             | `croma-linux-aarch64`       |
| windows-x86_64| `croma-lsp-windows-x86_64.exe`        | `croma-windows-x86_64.exe`  |

**The resolver as written already produces exactly these `croma-lsp-*` names and
already assumes uncompressed bare binaries** — so C2 needs **no resolver behavior
change**, only: (a) the workflow honoring the table, (b) dropping the
`// TODO(epic-C)` comment, (c) re-proving the extension host tests under the 1.96.0
toolchain. A CI assertion step (and/or the existing `asset_name` unit tests) pins
the produced names against the scheme as a single source of truth.

**Verification WITHOUT a public release (C2 gate):** the workflow gets a
`workflow_dispatch` path that builds the full matrix and uploads the binaries as
**Actions artifacts** (not a Release); only the `v*` tag path attaches to a real
GitHub Release. A manual `gh workflow run` dry-run on main thus proves all five
platforms build + names are correct with zero public/outward effect (no tag, no
release). Static lint via `actionlint`/`act` if available; host-platform build +
rename proven locally.

---

## 4. CHANGELOG — **Keep a Changelog**

`CHANGELOG.md` in [Keep a Changelog](https://keepachangelog.com/) format, SemVer.
The `1.0.0` entry is a **curated** summary of the shipped surface (the four
capabilities + grammar + Zed extension + benchmark suite), seeded from the
phase/PR history — not a raw PR dump. Subsequent releases append `Added/Changed/
Fixed/...` sections.

---

## 5. croma-core stays zero-dependency

Release work is metadata + CI only — **no runtime dependency is added**. The
`cargo tree -p croma-core --edges normal == 1 line` guard stays green and stays a
one-liner in `ci.yml` (`rust` job; deliberately NOT in the nix job — the dev-shell
banner breaks `cargo tree | wc -l`). `croma-core` remains crates.io-publishable
and dep-free.

---

## Stage plan (each on its own `feature/release-*` branch, landed via land.py)

- **C1** — metadata + version bump to `0.9.0` (per §1) + per-crate READMEs +
  `CHANGELOG.md` + `cargo publish --dry-run -p <crate>` for all four in dependency
  order. Gate: every dry-run packages cleanly; `cargo test/clippy/fmt --workspace`
  green; zero-dep guard = 1 line. **No publish.** Land via `land.py`.
- **C2** — tag-triggered binary-release CI honoring §3; reconcile/confirm the Zed
  resolver (drop TODO, re-prove host tests). Gate: workflow validated via
  `workflow_dispatch` artifact dry-run (no public release); asset names asserted ==
  resolver scheme. Land via `land.py`. Also write a `docs/releasing.md` runbook
  documenting the eventual 1.0 cut (so the steps are captured even though deferred).
- **C3 — DEFERRED (not done this milestone).** The irreversible public cut
  (`cargo publish`, public `vX.Y.Z` tag, public GitHub release, end-to-end
  auto-download verify) happens at the `1.0.0` public launch, on explicit user go.
  `docs/releasing.md` (written in C2) is the runbook for it.

## Proven gates that stay green (release work is not product logic)

`cargo test --workspace`; LSP legs A–E; fmt 10000/0; raw whitelist 9390/0; reader
9935/9935 + 98.50%; grammar `tree-sitter test` 17/17 + coverage ≥ 99.46%;
`clippy --all-targets -D warnings` + `fmt --check` clean; croma-core zero-dep;
workspace stays 4 members; `tree-sitter-abc/` + `editors/zed/` stay excluded.
