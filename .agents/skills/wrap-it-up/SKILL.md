---
name: wrap-it-up
description: Manual-only finalization for an existing Codex Git worktree. Use only when the user explicitly invokes `$wrap-it-up`; never use automatically or infer it from ordinary commit, merge, cleanup, or Git requests. Runs from inside a linked worktree to commit pending work, rebase the current branch onto local `master` when needed, fast-forward local `master` to the branch, then only after explicit user confirmation remove the worktree and delete the branch.
---

# Wrap It Up

## Overview

Use this skill to finish a Codex worktree session and clean up its branch. This skill is destructive at the final cleanup step, so run it only when explicitly invoked and keep the final removal behind a separate user confirmation.

## Workflow

1. Verify that the session is inside a linked Git worktree:

```bash
git rev-parse --show-toplevel
git rev-parse --git-common-dir
git rev-parse --absolute-git-dir
git status --short --branch
```

Stop if this is the main worktree, if the current branch is `master`, or if local `master` does not exist.

2. Choose a concise commit message.

- Prefer a message that reflects the actual work completed in the session.
- Ask the user for a message if the existing context is not enough to choose one accurately.
- If there are no pending changes, do not create an empty commit.

3. Run the finish phase from inside the linked worktree:

```bash
python3 <skill-dir>/scripts/wrap_it_up.py finish --message "<commit message>"
```

The finish phase must:

- Refuse to run if the current worktree already has a rebase, merge, cherry-pick, or revert in progress.
- Stage and commit all pending changes when the worktree is dirty.
- Rebase the current branch onto local `master` if `master` is not already an ancestor.
- Fast-forward local `master` to the completed branch from a clean worktree where `master` is checked out.
- Stop without cleanup if any command fails, including rebase conflicts, a dirty `master` worktree, or a non-fast-forward merge.

If a rebase conflict occurs, resolve it manually in the worktree, run `git add` for resolved files, and run `git rebase --continue`; or run `git rebase --abort` to abandon it. Re-run the finish phase only after Git has no interrupted operation in progress.

Do not fetch, pull, or interact with remotes unless the user explicitly asks.

4. Ask the user for explicit cleanup confirmation after the finish phase succeeds.

Use a direct question that names both the worktree path and branch. Do not auto-resolve this question.

5. If the user confirms, run the cleanup phase from a surviving worktree such as the `master` worktree:

```bash
python3 <skill-dir>/scripts/wrap_it_up.py cleanup --worktree "<worktree path>" --branch "<branch name>" --confirmed
```

The cleanup phase must verify that:

- The target worktree still exists and is clean.
- The target worktree is on the named branch.
- Local `master` contains the branch commit.
- The target worktree is not the `master` worktree.
- The target worktree has no rebase, merge, cherry-pick, or revert in progress.

Only then remove the worktree and delete the branch.

## Safety Rules

- Never run this skill unless the user explicitly invoked `$wrap-it-up`.
- Never delete a worktree or branch before the commit, rebase, and fast-forward steps have succeeded.
- Never skip the explicit cleanup confirmation.
- Never clean up after a failed or conflicted rebase.
- Never force-push, force-delete, reset hard, or overwrite user changes.
