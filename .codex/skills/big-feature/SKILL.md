---
name: big-feature
description: Clarify, scope, and shape large feature requests before implementation planning. Use only when the user explicitly asks to use big-feature, says they want to plan/scope/design a big feature, or provides a broad feature idea that needs collaborative refinement before code changes. Do not use for small fixes, ordinary implementation tasks, or code review.
---

# Big Feature

## Objective

Detail a broad feature request before making a thorough implementation plan for it.
Start from the user's terse feature description, discover relevant context, clarify
the desired behavior, and turn the idea into a concrete feature brief.

## Questions

Ask compact batches of high-value clarifying questions until there is a clear
understanding of the feature. Prefer questions that resolve product behavior,
scope, edge cases, constraints, acceptance criteria, and integration points.
Avoid asking questions that can be answered by inspecting the repository.

Offer recommendations or options when useful, and state the tradeoff behind each
option. If a decision has a reasonable default, propose it instead of leaving the
question fully open-ended.

## Workflow

- From the initial feature description, inspect the repository as needed to
  understand existing architecture, constraints, and likely integration points.
- Iterate with the user by asking focused questions and refining the feature.
- Do not implement code while using this skill unless the user explicitly asks to
  move from planning into implementation.
- Once the feature is clear, summarize the current understanding and ask whether
  to proceed to implementation planning.
- Proceed to the implementation plan only after the user confirms the feature
  brief is good enough or explicitly asks to continue.
- During implementation planning, ask additional clarifying questions when needed.

## Output

End the scoping phase with a concise feature brief containing:

- Goals and non-goals.
- User-facing behavior and important edge cases.
- Relevant architecture and integration points.
- Key decisions already made.
- Open questions or assumptions.
- Implementation slices or milestones.
- Main risks and validation strategy.
