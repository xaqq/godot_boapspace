---
name: refine-feature-doc
description: Iteratively refine markdown feature request documents into decision-complete feature briefs for later implementation planning. Use when Codex is given a feature doc, feature request markdown, partial spec, or explicit request to refine/scope a feature description; inspect the codebase for impact, identify missing business logic decisions, ask targeted follow-up questions across as many rounds as needed, resolve explicit help requests where possible, and produce or update a thorough feature document without implementing code.
---

# Refine Feature Doc

## Objective

Turn a feature request markdown file or pasted draft into a detailed feature
description that another agent can convert into an implementation plan from fresh
context. Prioritize correctness, completeness, explicit decisions, and clear
unknowns over speed.

Expect refinement to be interactive. The user may need to answer many product,
business-rule, UX, and edge-case questions before the document is complete. Ask
for those decisions instead of smoothing over uncertainty.

This is a refinement skill, not an implementation-planning skill. Do not produce
step-by-step code tasks unless the user explicitly asks for a plan.

## Core Workflow

1. Read the feature document completely. Preserve the user's intent, terminology,
   explicit uncertainties, and requested help areas.
2. Inspect the repository before asking questions. Identify relevant existing
   behavior, modules, scene/UI patterns, Rust simulation systems, bridge APIs,
   tests, and constraints.
3. Read `references/feature-doc-checklist.md` and use it as the completeness
   checklist. Add only checklist sections that matter to the feature.
4. Decide whether direct inspection is enough or whether to fan out subagents.
   Use subagents when the feature is broad, crosses unfamiliar subsystems, has
   subtle business-rule implications, or when independent gap-finding would
   materially improve correctness. Do not use subagents for narrow changes when
   local inspection is sufficient.
5. Resolve what can be resolved from code and project context. Do not invent
   business rules that are not implied by the document or repository. If the
   source document asks an explicit question, answer it only when repo evidence
   or the user's text supports the answer; otherwise label it as a recommended
   decision or an open decision.
6. Stop and ask targeted follow-up questions when missing decisions block a
   decision-complete brief. Expect multiple question rounds for broad features.
   Prefer focused batches grouped by topic, explain why each decision matters,
   and wait for the user's answers before treating the brief as complete.
7. Produce a refined feature document or patch the original document if the user
   gave a path and clearly asked for the file to be updated.

## Interaction Model

- Treat unresolved product behavior, simulation rules, UX choices, data
  ownership, persistence, and acceptance criteria as questions for the user.
- Ask after repository inspection, so questions are grounded in the existing
  codebase rather than speculation.
- Do not present a final feature brief while blocking decisions remain unless
  the user explicitly asks for a best-effort draft. In that case, mark unresolved
  decisions clearly.
- Keep each question batch actionable. Ask all questions needed for correctness,
  but split large sets into follow-up rounds when that makes answers easier.
- When the user answers, incorporate those decisions, inspect further if the
  answer changes the feature shape, and ask the next batch of blocking questions
  before finalizing.

## Repository Investigation

Inspect from the feature's likely integration points outward. Prefer `rg` and
targeted file reads. Gather evidence for:

- Existing user-facing behavior and constraints.
- Simulation data/components/systems/resources/events in `game_engine`.
- Godot bridge APIs and typed exported surface in `godot_bridge`.
- Godot scenes/resources/UI nodes that would present or trigger the behavior.
- Tests, fixtures, saved examples, and conventions the feature should preserve.

Avoid asking the user questions that the repository can answer. If repository
evidence conflicts with the draft, call out the conflict and ask for a decision
instead of silently rewriting the feature around one side.

## Optional Subagent Use

When subagents are useful, keep prompts narrow and evidence-oriented. Ask them to
inspect the codebase for impact, existing behavior, constraints, and missing
decisions for one subsystem or concern. Do not ask them to write the final brief.

Good subagent scopes:

- "Inspect the Rust simulation side for this feature and report existing systems,
  likely integration points, invariants, and unanswered product decisions."
- "Inspect the Godot bridge/UI side for this feature and report exposed APIs,
  scene/resource implications, and missing UX decisions."
- "Review this feature draft against existing tests and architecture; list
  correctness risks, ambiguous rules, and acceptance criteria gaps."

Use subagent findings as evidence, not authority. Reconcile conflicting reports
yourself and cite the files or modules that support important conclusions.

## Refinement Rules

- Keep durable game logic independent of Godot APIs. Put business rules in
  `game_engine`; keep `godot_bridge` thin.
- Preserve project constraints from AGENTS.md, including no GDScript, strong
  Godot typing, and tested Rust game logic.
- Separate decided behavior from recommended defaults and open questions.
- Do not let "assuming this default is acceptable" erase a decision. A default is
  either encoded as an explicit recommended decision with tradeoffs, or it remains
  an open decision before planning.
- Make edge cases explicit: invalid orders, missing resources, timing, entity
  lifecycle, multi-surface isolation, save/load implications, and UI feedback.
- Treat "help me refine this part" notes in the source document as first-class
  tasks. Either rewrite that section with a concrete recommendation or convert it
  into specific open decisions.
- Do not overfit the document to implementation details. Include architecture
  impact, contracts, and constraints, but leave implementation sequencing to a
  later plan.

## Output Shape

For a chat-only refinement, respond with:

1. `Refined Feature Document` containing the updated markdown.
2. `Key Decisions` listing decisions now encoded in the document.
3. `Open Decisions` listing only decisions that block a complete plan.
4. `Repository Evidence` listing the main files/modules inspected.

For a file update, edit the markdown document directly and then summarize:

- The sections added or changed.
- Remaining open decisions.
- Important repository evidence used.

The refined document should usually include:

- Summary and goals.
- Non-goals.
- User-facing behavior.
- Simulation/business rules.
- Data model, ECS, API, UI, scene, or bridge implications as relevant.
- Edge cases and failure behavior.
- Acceptance criteria.
- Tests and validation expectations.
- Open decisions and assumptions.
