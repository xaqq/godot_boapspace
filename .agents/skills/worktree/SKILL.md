---
name: worktree
description: Create or switch to repository-local Git worktrees for Codex sessions. Use only when the user explicitly invokes `$worktree` or explicitly asks to use the worktree skill for a session/worktree workflow; do not use automatically for ordinary Git, branching, or coding tasks.
---

# Worktree

## Overview

Use this skill to isolate a Codex session in its own Git worktree inside the repository directory. New worktrees must be based on the local `master` branch and, once created or selected, must become the working directory for the rest of the session.

## Workflow

1. Discover the repository top-level directory:

```bash
git rev-parse --show-toplevel
```

2. Inspect existing worktrees and current state:

```bash
git worktree list --porcelain
git status --short --branch
```

3. Choose a slug for the branch and directory:

- Prefer the current session rename/title if the session has been `/rename`d and the name is visible in context.
- Otherwise derive a short slug from the user's requested task.
- If no meaningful slug is available, ask the user for a worktree name.
- Sanitize to lowercase ASCII letters, digits, and hyphens.
- Use branch `codex/<slug>` and directory `<repo>/.worktrees/<slug>`.
- If either branch or directory already exists, append `-2`, `-3`, and so on until both are unused.

4. Verify the local base branch exists:

```bash
git rev-parse --verify master
```

If local `master` is missing, stop and ask the user which local branch to use. Do not silently use `origin/master` or another branch.

5. Keep repository-local worktrees out of the main worktree status by ensuring `.worktrees/` is listed in the Git common exclude file:

```bash
git rev-parse --git-common-dir
```

Add `.worktrees/` to `<git-common-dir>/info/exclude` only if it is not already present. Prefer this local exclude over editing the tracked `.gitignore` unless the user asks for a tracked ignore rule.

6. Create the worktree:

```bash
mkdir -p <repo>/.worktrees
git worktree add -b codex/<slug> <repo>/.worktrees/<slug> master
```

7. Verify the result:

```bash
git -C <repo>/.worktrees/<slug> status --short --branch
```

8. Switch the session to the worktree:

- Use `<repo>/.worktrees/<slug>` as the `workdir` for all subsequent shell commands.
- Read, edit, test, and commit files from that worktree path for the rest of the session.
- Tell the user the branch and absolute worktree path.

## Existing Worktrees

If the user asks to use an existing worktree, inspect `git worktree list --porcelain`, choose the matching entry, and switch subsequent session work to that path. Do not create a new branch or worktree unless the user asked for one.

## Safety

- Do not move, copy, stash, or apply dirty changes from the current worktree unless the user explicitly asks.
- Do not remove worktrees, prune worktrees, or delete branches unless the user explicitly asks.
- Do not run network operations such as `git fetch` unless the user explicitly asks or approves them.
- If a command fails because a target branch is already checked out, choose a new unique branch name rather than disturbing that worktree.
