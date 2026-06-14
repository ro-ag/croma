#!/usr/bin/env bash
# Croma branch lander: push a branch, open a PR, wait for green CI, squash-merge,
# delete the remote + local branch, and resync the default branch.
#
# Designed to automate the repetitive land-a-branch dance so it does not have to
# be driven step-by-step by an agent.
#
# Usage:
#   tools/land.sh [<branch>] [--no-test] [-y|--yes] [--interval N]
#
# Behavior:
#   - On the default branch (e.g. main): a <branch> argument is required; it must
#     exist locally and is checked out, then landed.
#   - On any other branch: the current branch is landed; a <branch> argument that
#     differs is ignored (with a warning).
#
# Flags:
#   --no-test       Skip the `cargo test --workspace` pre-push gate.
#   -y, --yes       Do not prompt for confirmation before landing.
#   --interval N    CI poll/watch interval in seconds (default 10).
#   -h, --help      Show this help.
#
# Requires: git, gh (authenticated). Idempotent-ish: reuses an existing open PR.

set -euo pipefail

# --- output helpers ---------------------------------------------------------
if [[ -t 1 ]]; then
  c_red=$'\e[31m'; c_grn=$'\e[32m'; c_yel=$'\e[33m'; c_rst=$'\e[0m'
else
  c_red=''; c_grn=''; c_yel=''; c_rst=''
fi
info() { printf '%s==>%s %s\n' "$c_grn" "$c_rst" "$*"; }
warn() { printf '%swarn:%s %s\n' "$c_yel" "$c_rst" "$*" >&2; }
die()  { printf '%serror:%s %s\n' "$c_red" "$c_rst" "$*" >&2; exit 1; }

usage() {
  awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "$0"
}

# --- parse args -------------------------------------------------------------
run_test=true
assume_yes=false
interval=10
arg_branch=""

while (( $# )); do
  case "$1" in
    --no-test)   run_test=false ;;
    -y|--yes)    assume_yes=true ;;
    --interval)  interval="${2:?--interval needs a value}"; shift ;;
    -h|--help)   usage; exit 0 ;;
    --)          shift; break ;;
    -*)          die "unknown flag: $1 (try --help)" ;;
    *)           [[ -z "$arg_branch" ]] || die "unexpected argument: $1"
                 arg_branch="$1" ;;
  esac
  shift
done
[[ "$interval" =~ ^[0-9]+$ && "$interval" -gt 0 ]] || die "--interval must be a positive integer"

# --- preconditions ----------------------------------------------------------
command -v git >/dev/null 2>&1 || die "git not found"
command -v gh  >/dev/null 2>&1 || die "gh not found (https://cli.github.com)"
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "not inside a git repository"
gh auth status >/dev/null 2>&1 || die "gh is not authenticated (run: gh auth login)"

# Block only tracked changes; untracked files (generated artifacts) are fine.
if ! git diff --quiet || ! git diff --cached --quiet; then
  die "working tree has uncommitted tracked changes; commit or stash them first"
fi

default_branch="$(gh repo view --json defaultBranchRef -q .defaultBranchRef.name 2>/dev/null || true)"
[[ -n "$default_branch" ]] || default_branch="main"

# --- choose the branch to land ---------------------------------------------
current="$(git branch --show-current)"
if [[ "$current" == "$default_branch" ]]; then
  [[ -n "$arg_branch" ]] || die "on '$default_branch': pass the branch to land, e.g. tools/land.sh feature/foo"
  git show-ref --verify --quiet "refs/heads/$arg_branch" \
    || die "branch '$arg_branch' not found locally"
  branch="$arg_branch"
  info "checking out '$branch'"
  git checkout "$branch"
else
  branch="$current"
  if [[ -n "$arg_branch" && "$arg_branch" != "$current" ]]; then
    warn "not on '$default_branch'; ignoring argument '$arg_branch', landing current branch '$current'"
  fi
fi
[[ -n "$branch" ]]                    || die "could not determine a branch (detached HEAD?)"
[[ "$branch" != "$default_branch" ]]  || die "refusing to land the default branch '$default_branch'"

# Make sure there is actually something to land.
git fetch --quiet origin "$default_branch" 2>/dev/null || true
ahead="$(git rev-list --count "origin/$default_branch..$branch" 2>/dev/null \
         || git rev-list --count "$default_branch..$branch" 2>/dev/null \
         || echo 1)"
[[ "$ahead" != "0" ]] || die "'$branch' has no commits ahead of '$default_branch'; nothing to land"

# --- test gate --------------------------------------------------------------
if [[ "$run_test" == true ]]; then
  info "running cargo test --workspace (skip with --no-test)"
  cargo test --workspace || die "tests failed; aborting land"
else
  warn "skipping test gate (--no-test)"
fi

# --- confirm ----------------------------------------------------------------
if [[ "$assume_yes" != true ]]; then
  printf '\n'
  printf 'Land plan:\n'
  printf '  branch : %s\n' "$branch"
  printf '  base   : %s\n' "$default_branch"
  printf '  merge  : squash, then delete remote + local branch\n'
  printf '  tests  : %s\n' "$([[ "$run_test" == true ]] && echo 'passed' || echo 'SKIPPED')"
  printf '\n'
  read -r -p "Proceed? [y/N] " ans
  [[ "$ans" =~ ^[Yy]$ ]] || die "aborted by user"
fi

# --- push -------------------------------------------------------------------
info "pushing '$branch' to origin"
git push -u origin "$branch"

# --- create or reuse PR -----------------------------------------------------
pr_num="$(gh pr view "$branch" --json number,state -q 'select(.state=="OPEN").number' 2>/dev/null || true)"
if [[ -z "$pr_num" ]]; then
  info "creating PR ($branch -> $default_branch)"
  gh pr create --fill --base "$default_branch" --head "$branch"
  pr_num="$(gh pr view "$branch" --json number -q .number)"
else
  info "reusing open PR #$pr_num"
fi
[[ -n "$pr_num" ]] || die "could not determine PR number"

# --- wait for CI to go green ------------------------------------------------
wait_for_checks() {
  local n="$1"
  local grace=90 waited=0 count

  # Checks can take a few seconds to register after the push/PR is created.
  while :; do
    count="$(gh pr checks "$n" --json state -q 'length' 2>/dev/null || echo 0)"
    [[ "$count" =~ ^[0-9]+$ ]] || count=0
    (( count > 0 )) && break
    if (( waited >= grace )); then
      warn "no CI checks reported after ${grace}s; proceeding without CI verification"
      return 0
    fi
    info "waiting for CI checks to register (${waited}s/${grace}s)..."
    sleep "$interval"; waited=$(( waited + interval ))
  done

  info "$count CI check(s) registered for PR #$n; watching until complete"
  gh pr checks "$n" --watch --fail-fast --interval "$interval" \
    || die "CI checks failed for PR #$n; not merging"
  info "CI is green"
}
wait_for_checks "$pr_num"

# --- merge ------------------------------------------------------------------
info "squash-merging PR #$pr_num and deleting the branch"
gh pr merge "$pr_num" --squash --delete-branch

# --- resync default branch + verify cleanup ---------------------------------
info "syncing '$default_branch'"
git checkout "$default_branch" 2>/dev/null || true
git pull --ff-only
git fetch --prune

# Safety net: if the branch is gone from the remote but a local copy survived,
# delete the local branch (matches the requested "delete branch if not on remote").
if git show-ref --verify --quiet "refs/heads/$branch"; then
  if git ls-remote --exit-code --heads origin "$branch" >/dev/null 2>&1; then
    warn "branch '$branch' still exists on the remote; leaving the local copy in place"
  else
    info "branch '$branch' gone from remote; deleting local branch"
    git branch -D "$branch"
  fi
fi

info "landed '$branch' into '$default_branch'. Done."
