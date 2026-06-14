#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["requests"]
# ///
"""Croma branch lander.

Push a branch, open a PR, wait for green CI, squash-merge, delete the remote +
local branch, and resync the default branch. Automates the repetitive
land-a-branch flow so it does not have to be driven step by step.

Usage:
    uv run tools/land.py [<branch>] [--no-test] [-y] [--interval N]
    ./tools/land.py ...          # same thing (shebang runs it via uv)

Behavior:
    * On the default branch: a <branch> argument is required; it must exist
      locally and is checked out, then landed.
    * On any other branch: the current branch is landed; a differing <branch>
      argument is ignored (with a warning).

CI is polled through the GitHub REST API keyed to the pushed commit SHA, so a
run superseded by a later push (a race that trips `gh pr checks --watch`) cannot
be mistaken for a failure -- only check runs for *this* commit are inspected.

Auth reuses the GitHub CLI token (`gh auth token`); run `gh auth login` once.
Requires: git, gh, and (unless --no-test) cargo.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import time

import requests

API = "https://api.github.com"
# Conclusions that mean a check genuinely failed. cancelled/skipped/neutral are
# tolerated: a cancelled run is usually one superseded by a newer push.
FAIL_CONCLUSIONS = {"failure", "timed_out", "action_required", "startup_failure", "stale"}

# --- output helpers ---------------------------------------------------------
_TTY = sys.stdout.isatty()


def _c(code: str) -> str:
    return code if _TTY else ""


RED, GRN, YEL, RST = _c("\033[31m"), _c("\033[32m"), _c("\033[33m"), _c("\033[0m")


def info(msg: str) -> None:
    print(f"{GRN}==>{RST} {msg}", flush=True)


def warn(msg: str) -> None:
    print(f"{YEL}warn:{RST} {msg}", file=sys.stderr, flush=True)


def die(msg: str, code: int = 1):
    print(f"{RED}error:{RST} {msg}", file=sys.stderr, flush=True)
    raise SystemExit(code)


# --- subprocess helpers -----------------------------------------------------
def run(cmd: list[str], *, check: bool = True, capture: bool = True) -> subprocess.CompletedProcess:
    res = subprocess.run(cmd, check=False, capture_output=capture, text=True)
    if check and res.returncode != 0:
        detail = ((res.stderr or "") + (res.stdout or "")).strip() if capture else ""
        die(f"command failed: {' '.join(cmd)}" + (f"\n{detail}" if detail else ""))
    return res


def git(*args: str, check: bool = True, capture: bool = True) -> subprocess.CompletedProcess:
    return run(["git", *args], check=check, capture=capture)


def git_out(*args: str) -> str:
    return git(*args).stdout.strip()


def have(tool: str) -> bool:
    return subprocess.run(["sh", "-c", f"command -v {tool}"], capture_output=True).returncode == 0


# --- GitHub REST client -----------------------------------------------------
class GitHub:
    def __init__(self, token: str):
        self.s = requests.Session()
        self.s.headers.update(
            {
                "Authorization": f"Bearer {token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
            }
        )

    def get(self, path: str, **kw) -> requests.Response:
        return self.s.get(API + path, timeout=30, **kw)


def gh_token() -> str:
    res = run(["gh", "auth", "token"], check=False)
    token = res.stdout.strip()
    if res.returncode != 0 or not token:
        die("could not obtain a GitHub token from `gh auth token` (run: gh auth login)")
    return token


def repo_slug() -> tuple[str, str]:
    url = git_out("remote", "get-url", "origin")
    m = re.search(r"github\.com[:/]+([^/]+)/(.+?)(?:\.git)?/?$", url)
    if not m:
        die(f"could not parse owner/repo from origin URL: {url}")
    return m.group(1), m.group(2)


def default_branch(gh: GitHub, owner: str, repo: str) -> str:
    resp = gh.get(f"/repos/{owner}/{repo}")
    if resp.status_code == 200 and resp.json().get("default_branch"):
        return resp.json()["default_branch"]
    res = run(
        ["gh", "repo", "view", "--json", "defaultBranchRef", "-q", ".defaultBranchRef.name"],
        check=False,
    )
    return res.stdout.strip() or "main"


def find_open_pr(gh: GitHub, owner: str, repo: str, branch: str) -> int | None:
    resp = gh.get(f"/repos/{owner}/{repo}/pulls", params={"head": f"{owner}:{branch}", "state": "open"})
    if resp.status_code == 200 and resp.json():
        return int(resp.json()[0]["number"])
    return None


# --- CI wait (keyed to the pushed SHA) --------------------------------------
def wait_for_checks(
    gh: GitHub, owner: str, repo: str, sha: str, interval: int, grace: int, timeout: int
) -> bool:
    """Poll check runs + commit statuses for `sha`.

    Returns True when CI is green (or no checks exist after the grace window),
    False on a real failure or timeout.
    """
    waited = 0
    ever_seen = False
    while True:
        runs: list[dict] = []
        resp = gh.get(f"/repos/{owner}/{repo}/commits/{sha}/check-runs")
        if resp.status_code == 200:
            runs = resp.json().get("check_runs", [])
        else:
            warn(f"check-runs query returned {resp.status_code}; retrying")

        # Legacy commit statuses (external CI); empty for Actions-only repos.
        statuses_state = None
        sresp = gh.get(f"/repos/{owner}/{repo}/commits/{sha}/status")
        if sresp.status_code == 200 and sresp.json().get("statuses"):
            statuses_state = sresp.json().get("state")  # success | pending | failure

        total = len(runs) + (1 if statuses_state else 0)
        if total > 0:
            ever_seen = True

        failed = [c for c in runs if c.get("conclusion") in FAIL_CONCLUSIONS]
        if failed or statuses_state == "failure":
            for c in failed:
                warn(f"  ✗ {c.get('name')}: {c.get('conclusion')}  {c.get('html_url', '')}")
            if statuses_state == "failure":
                warn("  ✗ commit status: failure")
            return False

        pending = [c for c in runs if c.get("status") != "completed"]
        still_pending = bool(pending) or statuses_state == "pending"

        if total > 0 and not still_pending:
            passed = [c for c in runs if c.get("conclusion") == "success"]
            if passed or statuses_state == "success":
                info(f"CI is green ({len(passed)} check(s) passed)")
                return True
            warn("checks completed but none passed (all cancelled/skipped); not merging")
            return False

        # Still waiting: pending, or no checks registered yet.
        if total == 0:
            if not ever_seen:
                if waited >= grace:
                    warn(f"no CI checks reported after {grace}s; proceeding without CI verification")
                    return True
                info(f"waiting for CI checks to register ({waited}s/{grace}s)...")
            else:
                # Saw checks before, none now: a push likely superseded the run.
                info("checks re-registering (run superseded?); waiting...")
        else:
            info(f"CI running ({len(pending)} pending / {len(runs)} check(s))...")

        if waited >= timeout:
            warn(f"timed out after {timeout}s waiting for CI")
            return False
        time.sleep(interval)
        waited += interval


# --- main flow --------------------------------------------------------------
def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        prog="land.py",
        description="Push a branch, open a PR, wait for green CI, squash-merge, and clean up.",
    )
    p.add_argument("branch", nargs="?", help="branch to land (required only when on the default branch)")
    p.add_argument("--no-test", action="store_true", help="skip the `cargo test --workspace` gate")
    p.add_argument("-y", "--yes", action="store_true", help="do not prompt for confirmation")
    p.add_argument("--interval", type=int, default=10, help="CI poll interval in seconds (default 10)")
    p.add_argument("--grace", type=int, default=90, help="seconds to wait for checks to first appear (default 90)")
    p.add_argument("--timeout", type=int, default=2400, help="max seconds to wait for CI (default 2400)")
    return p.parse_args()


def main() -> None:
    args = parse_args()
    if args.interval <= 0:
        die("--interval must be a positive integer")

    # Preconditions.
    if git("rev-parse", "--is-inside-work-tree", check=False).returncode != 0:
        die("not inside a git repository")
    if not have("gh"):
        die("gh not found (https://cli.github.com)")
    if not args.no_test and not have("cargo"):
        die("cargo not found (needed for the test gate; pass --no-test to skip)")
    if run(["gh", "auth", "status"], check=False).returncode != 0:
        die("gh is not authenticated (run: gh auth login)")

    # Block only tracked changes; untracked generated files are fine.
    dirty = (
        git("diff", "--quiet", check=False).returncode != 0
        or git("diff", "--cached", "--quiet", check=False).returncode != 0
    )
    if dirty:
        die("working tree has uncommitted tracked changes; commit or stash them first")

    owner, repo = repo_slug()
    gh = GitHub(gh_token())
    base = default_branch(gh, owner, repo)

    # Choose the branch to land.
    current = git_out("branch", "--show-current")
    if current == base:
        if not args.branch:
            die(f"on '{base}': pass the branch to land, e.g. tools/land.py feature/foo")
        if git("show-ref", "--verify", "--quiet", f"refs/heads/{args.branch}", check=False).returncode != 0:
            die(f"branch '{args.branch}' not found locally")
        branch = args.branch
        info(f"checking out '{branch}'")
        git("checkout", branch, capture=False)
    else:
        branch = current
        if args.branch and args.branch != current:
            warn(f"not on '{base}'; ignoring '{args.branch}', landing current branch '{current}'")

    if not branch:
        die("could not determine a branch (detached HEAD?)")
    if branch == base:
        die(f"refusing to land the default branch '{base}'")

    # Must be ahead of base.
    git("fetch", "--quiet", "origin", base, check=False)
    ahead = git("rev-list", "--count", f"origin/{base}..{branch}", check=False)
    if ahead.returncode == 0 and ahead.stdout.strip() == "0":
        die(f"'{branch}' has no commits ahead of '{base}'; nothing to land")

    # Test gate.
    if not args.no_test:
        info("running cargo test --workspace (skip with --no-test)")
        if subprocess.run(["cargo", "test", "--workspace"]).returncode != 0:
            die("tests failed; aborting land")
    else:
        warn("skipping test gate (--no-test)")

    # Confirm.
    if not args.yes:
        print(
            f"\nLand plan:\n"
            f"  branch : {branch}\n"
            f"  base   : {base}\n"
            f"  merge  : squash, then delete remote + local branch\n"
            f"  tests  : {'SKIPPED' if args.no_test else 'passed'}\n"
        )
        if input("Proceed? [y/N] ").strip().lower() != "y":
            die("aborted by user")

    # Push.
    info(f"pushing '{branch}' to origin")
    git("push", "-u", "origin", branch, capture=False)

    # Create or reuse the PR.
    pr = find_open_pr(gh, owner, repo, branch)
    if pr is None:
        info(f"creating PR ({branch} -> {base})")
        run(["gh", "pr", "create", "--fill", "--base", base, "--head", branch], capture=False)
        pr = find_open_pr(gh, owner, repo, branch)
        if pr is None:
            view = run(["gh", "pr", "view", branch, "--json", "number", "-q", ".number"], check=False)
            pr = int(view.stdout.strip()) if view.stdout.strip().isdigit() else None
    else:
        info(f"reusing open PR #{pr}")
    if not pr:
        die("could not determine the PR number")

    # Wait for CI on the exact pushed commit.
    sha = git_out("rev-parse", "HEAD")
    info(f"waiting for CI on PR #{pr} @ {sha[:8]}")
    if not wait_for_checks(gh, owner, repo, sha, args.interval, args.grace, args.timeout):
        die(f"CI is not green for PR #{pr}; not merging")

    # Merge (deletes remote + local branch and switches to base).
    info(f"squash-merging PR #{pr} and deleting the branch")
    run(["gh", "pr", "merge", str(pr), "--squash", "--delete-branch"], capture=False)

    # Resync the default branch and verify cleanup.
    info(f"syncing '{base}'")
    git("checkout", base, check=False, capture=False)
    git("pull", "--ff-only", capture=False)
    git("fetch", "--prune", capture=False)

    if git("show-ref", "--verify", "--quiet", f"refs/heads/{branch}", check=False).returncode == 0:
        on_remote = run(["git", "ls-remote", "--exit-code", "--heads", "origin", branch], check=False).returncode == 0
        if on_remote:
            warn(f"branch '{branch}' still exists on the remote; leaving the local copy in place")
        else:
            info(f"branch '{branch}' gone from remote; deleting local branch")
            git("branch", "-D", branch, capture=False)

    info(f"landed '{branch}' into '{base}'. Done.")


if __name__ == "__main__":
    main()
