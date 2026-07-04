---
name: plan-code-change
description: Rigorous planning for feature work and refactors in a codebase before implementation. Use when the user asks Codex to plan, evaluate, design, scope, or critique a new feature, refactor, architecture change, or implementation approach; especially when the user wants clarifying questions, flaws in their plan, alternatives, risk analysis, sequencing, or test strategy before code is changed.
---

# Plan Code Change

## Objective

Use this skill to plan a code change before implementation. Explore the repository, ground the plan in observed code, ask clarifying questions when requirements are underspecified, and actively look for flaws in the proposed approach. Offer concrete suggestions when the codebase gives enough evidence.

For this repository, read `references/godot-boapspace.md` after the initial file survey.

## Workflow

1. Restate the requested change in one sentence.
2. Inspect the repo before planning. Prefer `rg`, `rg --files`, targeted `sed`, and existing tests. Identify the relevant modules, ownership boundaries, public APIs, and current conventions.
3. Separate facts from open questions. Cite concrete files, types, functions, scene files, tests, or commands that support important observations.
4. Ask clarifying questions when they affect architecture, data ownership, player-visible behavior, persistence, performance, compatibility, testing, or scope. Keep questions targeted and explain why each one matters.
5. Critique the user's plan or the obvious implementation path. Look for misplaced ownership, duplicated state, leaky abstractions, weak typing, lifecycle hazards, untested behavior, data migration issues, cross-language friction, performance problems, and over-broad refactors.
6. Suggest a design when possible. Prefer the smallest design that fits the current architecture and leaves room for the next likely requirement.
7. Produce an implementation plan only after the exploration and critique. Keep it sequenced, testable, and tied to files or modules.

Avoid explicit assumptions as a planning substitute. If a missing decision changes the design, ask the question. If useful progress is still possible, present conditional options instead of choosing silently.

## Exploration Checklist

Use the relevant parts of this checklist:

- Locate entry points, facade APIs, resource ownership, and existing tests.
- Find similar features or older removed patterns to avoid reintroducing rejected designs.
- Check whether the requested behavior belongs in domain logic, bridge/integration code, UI, rendering, persistence, or tooling.
- Review naming and typing conventions near the target code.
- Check update loops, event/signal flow, lifecycle order, and error handling.
- Check whether the change affects serialization, scene files, editor wiring, build artifacts, generated files, or public APIs.
- Run or recommend the narrowest validation command that would prove the plan.

## Questions

Ask questions in small batches. Prioritize questions that can invalidate a plan:

- What behavior should the player or caller observe?
- Which layer should own the data or decision?
- Does this need to work across multiple surfaces, scenes, sessions, or save files?
- What are the expected scale and performance limits?
- What compatibility constraints exist for old data, existing scenes, or external APIs?
- What should happen on invalid input, missing references, or partial failure?
- Which tests or acceptance checks should define done?

When offering suggestions alongside questions, make the dependency clear:

```markdown
Question: Should construction orders persist per surface or globally? This decides whether the order queue belongs in `game_engine::simulation::SurfaceRuntime` or a higher-level coordinator.

Suggestion: If orders are surface-local, model them as ECS data inside each surface world and expose typed facade methods through `GameSimulation`.
```

## Critique

Always include a "Plan Flaws / Risks" section when the user provides a plan or when the obvious implementation has meaningful risks. Be direct and specific:

- Name the flawed design choice.
- Explain the failure mode.
- Point to the code boundary or project rule it conflicts with.
- Offer a safer alternative when one is visible.

Do not invent flaws just to fill the section. If no serious flaw is visible after exploration, say so and list residual unknowns.

## Output Shape

Use this structure unless the user asks for a different format:

```markdown
**Understanding**
[One or two sentences.]

**What I Found**
[Repo-grounded facts with file references.]

**Clarifying Questions**
[Targeted questions with why they matter.]

**Plan Flaws / Risks**
[Concrete critique and alternatives.]

**Recommended Direction**
[Design recommendation, or conditional options if a key answer is missing.]

**Implementation Plan**
[Ordered steps, each tied to files/modules.]

**Validation**
[Tests, commands, manual checks, and gaps.]
```

If the next useful action is to ask questions before planning, stop after `Clarifying Questions`, plus any low-risk suggestions already supported by the code.

## Boundaries

- Do not edit code while using this skill unless the user explicitly asks to move from planning to implementation.
- Do not rely on stale memory of the project when files are available.
- Do not recommend broad rewrites unless the current architecture blocks the goal and a narrower option would create worse long-term cost.
- Do not place game simulation state in UI or Godot bridge code when it belongs in the engine layer.
- Do not push Godot-specific types into pure Rust engine code.
- Do not turn unanswered requirements into hidden assumptions.
