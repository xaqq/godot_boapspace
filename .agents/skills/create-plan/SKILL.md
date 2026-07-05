---
name: create-plan
description: Used when creating an implementation plan. The agent should switch to plan mode.
---

# Create Plan

## Objective

Create a thorough implementation plan for a feature, code change, or refactor.
It should be possible to implement the plan from a different context, so it needs to be verified: avoid
assumptions and try to verify them or ask instead.

## Workflow

Inspect the repository and iterate with the user to refine the plan until we have a clear shared
understanding of the plan.

Ask batches of high-value clarifying questions until there is a clear
understanding of the plan.
Avoid asking questions that can be answered by inspecting the repository.

Offer recommendations or options when useful, and state the tradeoff behind each
option. If a decision has a reasonable default, propose it instead of leaving the
question fully open-ended.

## Output

Once we have a detailed implementation, ask the user if this should be saved in the repository
(`saved_plans` directory) or we should proceed with implementation.
