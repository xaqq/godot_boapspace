#!/usr/bin/env python3
"""Finish and optionally clean up a linked Git worktree."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


class WrapError(RuntimeError):
    pass


@dataclass(frozen=True)
class Worktree:
    path: Path
    head: str | None
    branch_ref: str | None

    @property
    def branch(self) -> str | None:
        if self.branch_ref and self.branch_ref.startswith("refs/heads/"):
            return self.branch_ref.removeprefix("refs/heads/")
        return None


def run(
    args: list[str],
    *,
    cwd: Path | None = None,
    capture: bool = False,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    display = " ".join(args)
    location = f" (cwd={cwd})" if cwd else ""
    print(f"+ {display}{location}")
    completed = subprocess.run(
        args,
        cwd=str(cwd) if cwd else None,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    if check and completed.returncode != 0:
        details = ""
        if capture:
            details = "\n" + (completed.stdout or "") + (completed.stderr or "")
        raise WrapError(f"command failed: {display}{details}")
    return completed


def git(args: list[str], *, cwd: Path | None = None, capture: bool = True) -> str:
    return run(["git", *args], cwd=cwd, capture=capture).stdout.strip()


def git_ok(args: list[str], *, cwd: Path | None = None) -> bool:
    return run(["git", *args], cwd=cwd, capture=True, check=False).returncode == 0


def git_path(repo: Path, path: str) -> Path:
    resolved = Path(git(["rev-parse", "--git-path", path], cwd=repo))
    if resolved.is_absolute():
        return resolved
    return (repo / resolved).resolve()


def parse_worktrees(repo: Path) -> list[Worktree]:
    output = git(["worktree", "list", "--porcelain"], cwd=repo)
    entries: list[Worktree] = []
    current: dict[str, str] = {}

    def flush() -> None:
        if "worktree" in current:
            entries.append(
                Worktree(
                    path=Path(current["worktree"]).resolve(),
                    head=current.get("HEAD"),
                    branch_ref=current.get("branch"),
                )
            )
        current.clear()

    for line in output.splitlines():
        if not line:
            flush()
            continue
        key, _, value = line.partition(" ")
        current[key] = value
    flush()
    return entries


def current_repo() -> Path:
    return Path(git(["rev-parse", "--show-toplevel"])).resolve()


def is_linked_worktree(repo: Path) -> bool:
    git_dir = Path(git(["rev-parse", "--absolute-git-dir"], cwd=repo)).resolve()
    common_dir = Path(git(["rev-parse", "--git-common-dir"], cwd=repo)).resolve()
    return git_dir != common_dir


def current_branch(repo: Path) -> str:
    branch = git(["branch", "--show-current"], cwd=repo)
    if not branch:
        raise WrapError("current worktree is detached; expected a named branch")
    return branch


def require_clean(repo: Path, label: str) -> None:
    status = git(["status", "--porcelain"], cwd=repo)
    if status:
        raise WrapError(f"{label} is not clean:\n{status}")


def require_no_interrupted_operation(repo: Path, label: str) -> None:
    markers = [
        ("rebase-merge", "a rebase is in progress"),
        ("rebase-apply", "a rebase or patch application is in progress"),
        ("MERGE_HEAD", "a merge is in progress"),
        ("CHERRY_PICK_HEAD", "a cherry-pick is in progress"),
        ("REVERT_HEAD", "a revert is in progress"),
    ]
    for marker, description in markers:
        if git_path(repo, marker).exists():
            raise WrapError(
                f"{label} has an interrupted Git operation: {description}. Resolve it "
                "and continue or abort it before running wrap-it-up again."
            )


def find_master_worktree(worktrees: list[Worktree]) -> Worktree:
    for worktree in worktrees:
        if worktree.branch == "master":
            return worktree
    raise WrapError("local master must be checked out in a worktree to fast-forward it safely")


def find_worktree(worktrees: list[Worktree], path: Path) -> Worktree:
    target = path.resolve()
    for worktree in worktrees:
        if worktree.path == target:
            return worktree
    raise WrapError(f"{target} is not registered as a Git worktree")


def commit_pending_changes(repo: Path, message: str | None) -> None:
    status = git(["status", "--porcelain"], cwd=repo)
    if not status:
        print("No pending changes to commit.")
        return
    if not message:
        raise WrapError("pending changes exist; provide --message for the commit")
    run(["git", "add", "-A"], cwd=repo)
    run(["git", "commit", "-m", message], cwd=repo)


def rebase_onto_master_if_needed(repo: Path) -> None:
    if git_ok(["merge-base", "--is-ancestor", "master", "HEAD"], cwd=repo):
        print("Current branch already contains master; no rebase needed.")
        return
    run(["git", "rebase", "master"], cwd=repo)


def fast_forward_master(master_repo: Path, branch: str) -> None:
    master_branch = current_branch(master_repo)
    if master_branch != "master":
        raise WrapError(f"master worktree is on {master_branch!r}, expected 'master'")
    require_clean(master_repo, "master worktree")
    run(["git", "merge", "--ff-only", branch], cwd=master_repo)


def finish(args: argparse.Namespace) -> None:
    repo = current_repo()
    if not is_linked_worktree(repo):
        raise WrapError("run finish from inside a linked worktree, not the main worktree")
    require_no_interrupted_operation(repo, "current worktree")

    branch = current_branch(repo)
    if branch == "master":
        raise WrapError("run finish from a feature worktree branch, not master")
    if not git_ok(["rev-parse", "--verify", "master"], cwd=repo):
        raise WrapError("local master branch does not exist")

    worktrees = parse_worktrees(repo)
    current = find_worktree(worktrees, repo)
    master = find_master_worktree(worktrees)
    if master.path == current.path:
        raise WrapError("current worktree is the master worktree")

    commit_pending_changes(repo, args.message)
    require_clean(repo, "current worktree")
    rebase_onto_master_if_needed(repo)
    require_clean(repo, "current worktree after rebase")
    fast_forward_master(master.path, branch)

    print("\nFinish phase succeeded.")
    print(f"Branch: {branch}")
    print(f"Worktree: {current.path}")
    print(f"Master worktree: {master.path}")
    print("\nAsk the user before cleanup. If confirmed, run:")
    script = Path(__file__).resolve()
    print(
        "python3 "
        f"{script} cleanup --worktree {quote_arg(current.path)} "
        f"--branch {quote_arg(branch)} --confirmed"
    )


def quote_arg(value: object) -> str:
    text = str(value)
    return "'" + text.replace("'", "'\"'\"'") + "'"


def cleanup(args: argparse.Namespace) -> None:
    if not args.confirmed:
        raise WrapError("cleanup requires --confirmed after explicit user confirmation")

    target = Path(args.worktree).resolve()
    if not target.exists():
        raise WrapError(f"worktree path does not exist: {target}")
    if not git_ok(["rev-parse", "--is-inside-work-tree"], cwd=target):
        raise WrapError(f"not a Git worktree: {target}")
    require_no_interrupted_operation(target, "target worktree")

    branch = args.branch
    repo = Path(git(["rev-parse", "--show-toplevel"], cwd=target)).resolve()
    worktrees = parse_worktrees(repo)
    target_info = find_worktree(worktrees, target)
    master = find_master_worktree(worktrees)

    if target_info.branch != branch:
        raise WrapError(f"target worktree is on {target_info.branch!r}, expected {branch!r}")
    if branch == "master" or target_info.path == master.path:
        raise WrapError("refusing to remove the master worktree")
    require_clean(target, "target worktree")
    if not git_ok(["merge-base", "--is-ancestor", branch, "master"], cwd=target):
        raise WrapError(f"master does not contain branch {branch!r}; cleanup refused")

    os.chdir(master.path)
    run(["git", "worktree", "remove", str(target)], cwd=master.path)
    run(["git", "branch", "-d", branch], cwd=master.path)
    print("Cleanup phase succeeded.")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    finish_parser = subparsers.add_parser("finish", help="commit, rebase, and fast-forward master")
    finish_parser.add_argument("--message", help="commit message to use when pending changes exist")
    finish_parser.set_defaults(func=finish)

    cleanup_parser = subparsers.add_parser("cleanup", help="remove a finished worktree and branch")
    cleanup_parser.add_argument("--worktree", required=True, help="target worktree path to remove")
    cleanup_parser.add_argument("--branch", required=True, help="target branch to delete")
    cleanup_parser.add_argument(
        "--confirmed",
        action="store_true",
        help="required after explicit user confirmation",
    )
    cleanup_parser.set_defaults(func=cleanup)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    try:
        args.func(args)
    except WrapError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
