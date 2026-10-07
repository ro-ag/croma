# AGENTS.md

Guidance for AI agents (Claude Code, Codex, etc.) working in **croma** — the Rust
toolkit repository. Read this first, every session.

## Two repositories

croma is split in two:

- **croma** (this repo) — the toolkit: `croma-core`, `croma-cli`, `croma-fmt`,
  `croma-lsp`, the reusable `tree-sitter-abc` grammar, and the Zed extension. It
  builds and tests standalone. This is what a developer clones to build, fix, and
  ship.
- **croma-test** (private, <https://github.com/ro-ag/croma-test>) — the
  corpus-scale proving suite: the Python provers, the 10k ABC corpus, the abc2xml
  comparator + whitelist/dropped baseline, the ABC 2.1 spec knowledge base + the
  divergence-triage tooling, the progress tracker, and the full design-decisions
  trail (`specs/`). Clone it only to prove croma at corpus scale or to track the
  project.

The dependency is one-way: **croma-test depends on croma**, never the reverse.
`croma-core` stays zero-dependency and crates.io-publishable.

## Start every session here

```sh
tools/session_bootstrap.sh              # git state + toolchain + build croma
tools/session_bootstrap.sh --with-suite # also clone/update ./croma-test/ (the suite)
```

Bootstrap reports git state, provisions the pinned Rust toolchain, and builds
`target/debug/croma`. With `--with-suite` it clones the private croma-test repo
into the git-ignored `./croma-test/` for corpus-scale gate runs.

## Environment

Two interchangeable environments, same pinned toolchain (details:
[`docs/development-environment.md`](docs/development-environment.md)):

- **Linux cloud sandbox** — provisioned with `rustup`. Ephemeral: commit and push
  anything worth keeping.
- **Local Nix flake** — `nix develop` / direnv, any OS.

Rust 1.96.0 is pinned by `rust-toolchain.toml`; plain `cargo`/`rustc` select it on
any host. Never hardcode an absolute toolchain path.

## Standing rules

- **Never work on `main`.** Branch per change by type: `feature/<slug>`,
  `bugfix/<slug>`, or `refactor/<slug>`.
- `croma-core` must stay crates.io-publishable and zero-dependency — no
  path-only/local runtime assumptions in library code. The MusicXML reader's only
  dependency (`roxmltree`) is opt-in via the `croma-core` `musicxml-reader` feature
  and ships on the CLI binary, never the library default. Build a reader-less CLI
  with `cargo build -p croma-cli --no-default-features`.
- The cloned `./croma-test/` subdir is git-ignored. Never commit it, or its
  generated artifacts, into croma.
- The four capabilities are **promoted (un-gated)** and ship by default:
  - **Formatter** (`croma fmt` / `--auto-fix`): canonical ABC pretty-printer,
    idempotent + lossless over the 10k corpus — [`docs/formatter.md`](docs/formatter.md).
  - **MusicXML→ABC reader** (`croma read` / `croma musicxml2abc`): inverts croma's
    own writer (self-loop 9933/9935) and reads foreign MusicXML (abc2xml/MuseScore/
    Finale/Sibelius) at 98.50% music21 parity — [`docs/musicxml-reader.md`](docs/musicxml-reader.md).
  - **LSP** (`croma-lsp`): a thin stdio adapter over the core/formatter —
    diagnostics + formatting byte-identical to the core, ~1 ms latency —
    [`docs/lsp.md`](docs/lsp.md).
  - **Editors**: the reusable `tree-sitter-abc` grammar + Zed extension —
    [`docs/editors.md`](docs/editors.md).
  Any LSP/reader-vs-core mismatch is a bug, not a new spec; re-prove the relevant
  legs (in croma-test) after any touch.

## Parser recovery policy

The parser is **strict to ABC 2.1**. When it meets malformed input it follows one
three-tier rule (loose source is the formatter's job, not the parser's):

1. **Default: reject.** Input that does not match the spec grammar is not silently
   accepted.
2. **Recover *and warn* — only for a clear intention spoiled by a minimal,
   mechanical slip** (a stray space/comma, a missing space). Recover the obvious
   intent and **always emit a diagnostic** — recovery is **never silent**. A silent
   recovery is indistinguishable from mimicking `abc2xml`; the warning is what makes
   recovery defensible as transparent strict-recognition.
3. **Otherwise: strict reject.** If the intention is not unambiguous, or the mistake
   is not a trivial slip, reject it. Repair belongs in `croma fmt --auto-fix`, which
   sanitises loose source into canonical spelling the strict parser then reads
   cleanly.

Corpus impact: warnings are stderr diagnostics, so adding one is **whitelist-neutral**
(the MusicXML is unchanged) and always safe to land. Converting a recovery into a
reject **changes the MusicXML**, so it can drop files out of the whitelist; that is
acceptable only as an **adjudicated** drop (croma is strict-correct, `abc2xml` is
lenient), never a silent regression. The comparator, the whitelist/dropped baseline,
the ABC spec KB, and the divergence-triage process all live in **croma-test**
(`comparison/abc2xml-divergences/`); triage each such file there, one at a time.

## Validate before committing

```sh
cargo test --workspace                              # Rust unit + integration tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo run -p croma-cli -- xml examples/basic.abc
```

Corpus-scale proofs (fmt 10k, reader 10k, LSP legs A–E, abc2xml whitelist 9390/0,
grammar coverage) run from **croma-test** — see its README. `cargo test` here skips
them cleanly when the corpus is absent (they are `ABC_ROOT`-gated).

## Landing

`uv run tools/land.py <branch>` is the standard push → PR → green-CI → squash-merge
→ cleanup flow. Open a pull request only when validation passes and the user asks
for one.

## Progress tracking & decisions trail

The progress tracker, the per-phase ledger, and the full design-decisions trail
(`specs/`) live in **croma-test** (`progress/`, `specs/`). Consult them there for
project history or the rationale behind a capability or promotion.

<!-- ptrack:begin -->
## ptrack — session context

This project uses `ptrack` to persist planning state so a fresh agent can
resume after a previous session grew too large.

**At session start** — reload context:
- `ptrack context` — goal, summary, active plan, open tasks, blockers, open issues, inventory (add `--json` to parse).

**If the project is empty** — populate it from this repo (README, docs, code, git
log, open issues), then keep it current:
- Goal: `ptrack goal set "north star"`
- Milestones (checkpoints): `ptrack milestone add "v1.0" [--due YYYY-MM-DD]`
- Plans (workstreams): `ptrack plan add "..." [--milestone N]`, then `ptrack plan use N` (also claims it). Junk plans are removed with `ptrack plan delete <id> --force` (preview first without `--force`), and work that belongs elsewhere moves with `ptrack plan move <id> --to <project>`.
- Tasks with status: `ptrack task add "..." [--plan N]` then `task start` (in progress) / `task done` / `task block` (todo = pending)
- Issues (bugs/problems): `ptrack issue add "..." --body "reproduction, expected/observed behavior, evidence" [--severity high] [--task N]`
- Maintain reports with `ptrack issue edit <id> --body "..." [--title "..."] [--severity high] [--status open|closed]`.
- Unscheduled issues are triage context, not permission to start work. Only on explicit user direction, `ptrack issue schedule <id> --plan N` creates and links a normal task; `issue link <id> --task N` relinks existing work; `issue unlink <id>` clears only the link. Task completion and issue closing are independent.
- Decisions: `ptrack note add "..." [--task N | --plan N]`

**Titles are names, not status.** Do not prefix titles with "Pending:", "In
progress:", "Done:", etc. — ptrack tracks status separately. Set it with
`task start|done|block`, `plan done|use`, `milestone done`, `issue close`. Rename with
`ptrack <plan|task|milestone|issue> rename <id> "new title"`.

**Pausing work.** A plan or task waiting on something external goes on hold with
a reason, independently of its status: `ptrack task hold <id> "waiting on review"`
/ `ptrack task resume <id>` (same for `plan hold|resume`). Completing the item
clears its hold too. Do not pick up a held item; `ptrack next` skips them.

**Ordering work.** When one item must wait for another, record the edge:
`ptrack task dep add <id> <dep-id>` (the first id waits on the second; tasks
in different plans are fine, and `plan dep add` does the same between plans).
`ptrack next` skips dep-blocked tasks and names the blockers; `ptrack context`
lists waiting work separately. `dep remove` deletes an edge, `dep list <id>`
shows them. Self-deps, duplicates, and cycles are refused.

**Working with other developers.** Configure your identity once per machine:
`ptrack config set user "<your name>"` (a stable ID is minted the first time;
renaming later keeps it). `ptrack plan use <id>` then claims the plan for you
as well as making it your active plan; content changes to a plan claimed by
someone else are refused. Holds, notes, and issue links stay open to everyone
— use them to talk across a claim. `ptrack plan release <id>` frees your
claim, finishing a plan releases it automatically, and
`ptrack plan use <id> --steal` takes over someone else's claim.

**Record decisions, not narration.** Notes are the evidence record and the
human-visible audit trail of what you did and *why*: one event per note, with
the exact identifiers, hashes, and counts. When you make a choice, hit a
blocker, or find a constraint, capture it —
`ptrack note add "chose X over Y because Z" --task N`. Terse and
identifier-heavy is correct here; that is what notes are for. Do not log
routine steps, tool output, or restate the code, and write with ordinary
spacing — squeezing the spaces out of "95 started, 95 validated, 95 closed"
makes the tokens no cheaper and the line harder to scan.

**Commits are tracked.** Reference the task in commit messages as `#<id>` so the
commit links to it (`ptrack hook install` records commits automatically; each
commit's `#<id>` links it to that task, otherwise the active plan).

**Closing work is gated.** `ptrack task done <id> --summary "what changed,
where it is wired in, what remains"` — the summary is required and the task
must have at least one linked commit (`#<id>` in the commit message, or
`ptrack commit record`); otherwise `task done` errors. Building a feature is
not done until something calls it — the summary must answer "what calls this
now?".

**One task in progress at a time.** Finish the started task properly, or park
it (`task hold`/`task block` with a reason), before `task start`, `task add`,
or `plan add` — they error while a started task is unfinished.

**Plans close through their checkpoint.** Every new plan ends with an
auto-added "Integrate and verify" task, and `plan done` errors while any task
is open. After every `plan done`, act on the printed CHECKPOINT block:
re-evaluate the remaining roadmap against the goal, refresh
`ptrack summary set`, and add or adjust plans and issues. `ptrack checkpoint`
re-prints the block on demand.

**`--force` is an audited exception.** Each gate accepts `--force` for genuine
exceptions (abandoned work, external changes); every use is recorded as a note
on the record. Do not use it to skip the workflow.

**Before ending** — save the narrative for the next agent with
`ptrack summary set "where we are"`. The rolling summary is the handoff
narrative: it answers "where does this project stand" for an agent or a person
arriving cold. Write 2-4 sentences, around 400 characters and under 1000 bytes.
Do not concatenate recent notes into it — if a reader would need the note to
decode the sentence, it belongs in the note, not the summary. Ordinary spacing
and ordinary sentences here too.

**Query on demand** (all bounded, `--json` available):
- `ptrack next` · `ptrack board` · `ptrack milestone list` · `ptrack plan show <id>` · `ptrack task show <id>` · `ptrack task list --status doing,blocked` · `ptrack issue list` · `ptrack search <term>` · `ptrack note list`

If no project exists yet: `ptrack init --goal "..."`.

---

## Working agreements

Standing rules for any agent working in this project (from ~/dev/ai):

- **Branch first.** Never commit to `main`/`master`. Land work via PR + squash
  merge; leave only `main` behind in local and remote.
- **No AI attribution** in commits, PRs, or release notes — no `Co-Authored-By`,
  no "Generated with …".
- **Stay in scope.** Do not refactor unrelated code, modify unrelated files, or
  add dependencies without approval.
- **Releases only on explicit request**, and only via CI on tag push — never a
  local publish. Keep tag, changelog, and README consistent; tests green first.
- **CI stays cheap.** No new workflows without an explicit request; triggers on
  merge to `main` / release tags only. When CI exists or is requested: lint and
  portable unit tests on Linux only, Windows gated to PRs + `main`, macOS
  UI/AppKit tests gated to approved PRs / `main` / nightly / releases. Cancel
  superseded PR runs (`concurrency`), filter paths, cache dependencies, and
  make expensive jobs `needs:` the cheap Linux checks first.
- **No repo or no remote → stop and ask** before making changes.
<!-- ptrack:end -->
