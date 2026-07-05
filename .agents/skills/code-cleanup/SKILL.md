---
name: code-cleanup
description: Plan behavior-preserving code cleanup refactors. Use when the user asks to clean up, simplify, reorganize, remove bad practice, improve maintainability, or prepare a refactor plan without changing product behavior.
---

# Code Cleanup

## Objective

Plan code cleanup that improves maintainability without changing observable behavior.

## Rules

- Treat cleanup as behavior-preserving unless the user explicitly asks for behavior changes. Call out any proposed behavior change separately.
- Do not edit code while using this skill. Stop at an implementation plan until the user approves it.
- Keep scope tied to the area or problem the user named. Avoid unrelated refactors.
- Prefer existing patterns, ownership boundaries, public contracts, and local style.
- Add abstractions only when they remove real complexity, reduce meaningful duplication, or match an established local pattern.
- Inspect existing tests and propose additions or adjustments when the cleanup has meaningful risk.

## Workflow

- Identify the target area and suspected problems. Ask the user only when scope or intent is ambiguous or risky to assume.
- Inspect relevant code and tests. Distinguish confirmed issues from style preferences or unproven concerns.
- Evaluate whether cleanup is worthwhile. For tiny or low-value concerns, recommend no change or a narrower cleanup.
- Investigate practical improvement options. If multiple approaches are viable, present pros and cons, recommend one, and ask the user to choose when the tradeoff matters.
- Use subagents for broad, cross-module, risky, or ambiguous cleanup investigations. Skip them for small localized cleanups.
- When the user agrees on an approach, create a detailed implementation plan.
- Use subagents to adversarially review the proposed plan when the scope or risk justifies it.
- Revise the plan based on review findings and user feedback.
- After plan approval, ask the user whether they want to continue to implementation or save the plan for future use.

## Plan Content

Include:

- Problem statement and scope
- Behavior-preservation constraints
- Proposed changes in implementation order
- Test and verification plan
- Risks, migration notes, or rollback notes when relevant
- Explicit non-goals
