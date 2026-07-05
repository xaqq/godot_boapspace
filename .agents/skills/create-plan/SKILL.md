---
name: create-plan
description: Create verified implementation plans for features, code changes, or refactors. Use when Codex is asked to plan an implementation, scope work before coding, save a plan, or switch into plan mode; while active, inspect first, ask targeted questions, and do not implement code until the user explicitly approves implementation.
---

# Create Plan

## Objective

Create a verified implementation plan for a feature, code change, or refactor.
Make the plan detailed enough for another agent to implement from fresh context.
Verify assumptions from repository context when possible; ask instead when a
decision cannot be answered from inspection.

## Workflow

Inspect the repository before asking questions. Identify relevant files, modules,
tests, patterns, constraints, and prior decisions.

Iterate with the user until there is a clear shared understanding of the plan.
Ask 1-4 clarifying questions at a time when needed.
Avoid asking questions that can be answered by inspecting the repository.

Offer recommendations or options when useful, and state the tradeoff behind each
option. If a decision has a reasonable default, propose it instead of leaving the
question fully open-ended.

Do not change product code while planning. Only save the plan to the repository
after the user chooses that output.

## Plan Contents

Include:

- Goal and current context
- Non-goals or explicitly deferred work
- Relevant files, modules, and existing patterns
- Proposed implementation steps in order
- Data, API, UI, scene, or contract changes when relevant
- Tests and validation commands
- Risks, edge cases, and assumptions
- Open questions, if any
- Acceptance criteria

## Output

Once we have a detailed implementation plan, present it in chat and ask the user
which final action to take:

- Save the plan in the repository under `saved_plans/YYYY-MM-DD-short-slug.md`
- Proceed with implementation
- Stop with the plan in chat
