# CLAUDE.md

This project's agent guidance lives in [`AGENTS.md`](AGENTS.md). Read it first,
every session, and run `tools/session_bootstrap.sh` before doing anything else.

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
