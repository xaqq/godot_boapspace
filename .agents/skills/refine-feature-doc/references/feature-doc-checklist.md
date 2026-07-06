# Feature Document Completeness Checklist

Use this checklist to find gaps in a feature request. Include only sections that
matter to the feature being refined.

## Product Behavior

- State the player-visible goal in one or two sentences.
- Define who or what triggers the feature.
- Define success, partial success, failure, cancellation, and retry behavior.
- Identify non-goals and intentionally deferred behavior.
- Specify how the player sees state, progress, errors, and completion.

## Business Rules

- Define all inputs, outputs, preconditions, and invariants.
- Define priority, ordering, conflict resolution, and tie-breaking rules.
- Define timing: instant, per tick, scheduled, cooldown, duration, or async.
- Define resource costs, refunds, reservations, ownership, and scarcity behavior.
- Define entity lifecycle effects: creation, deletion, movement, damage, death,
  assignment, unassignment, or state transitions.
- Define what happens when dependencies disappear mid-action.

## Simulation And ECS

- Identify affected components, resources, systems, events, and queries.
- Define whether behavior is per entity, per area/surface, or global.
- Preserve isolation between surfaces, areas, or planets unless the feature
  explicitly crosses boundaries.
- Define deterministic behavior expectations when order matters.
- Define how the feature interacts with existing orders, jobs, tasks, AI, pathing,
  inventory, construction, production, or colony state.

## Godot Bridge And UI

- Define what Godot needs to query, command, or subscribe to through the bridge.
- Keep durable rules in Rust simulation code; expose only thin bridge methods and
  serialized view data to Godot.
- Identify scenes, resources, controls, overlays, inspectors, or input handling
  affected by the feature.
- Define UI state for unavailable actions, invalid selections, empty states,
  progress, errors, and completion.
- Prefer typed Godot references and method calls; avoid stringly node/method
  access except where Godot APIs require it.

## Data, Persistence, And Compatibility

- Define any new serialized state or save/load behavior.
- Define migration or default behavior for existing saves if relevant.
- Define IDs, references, ownership, and lifetime rules.
- Define whether data belongs in the simulation, bridge DTOs, Godot resources, or
  imported assets.

## Edge Cases

- Invalid command target.
- Missing entity, resource, surface, area, or selection.
- Duplicate command.
- Conflicting simultaneous commands.
- Paused simulation or variable tick rate.
- Entity removed or transformed while work is in progress.
- UI opened while state changes underneath it.
- Empty colony, full inventory/storage, blocked path, or unavailable worker.

## Acceptance Criteria

- List observable outcomes a user or test can verify.
- Include at least one happy path and the important failure paths.
- Include simulation-level test expectations when durable logic changes.
- Include bridge/UI validation expectations when Godot-facing behavior changes.

## Open Decisions

Only leave an open decision when it cannot be answered from the draft or
repository evidence. During refinement, ask the user to decide blocking open
decisions before finalizing the brief. Phrase each as a decision the user can
answer, not as a vague concern. When a default is recommended, state the
tradeoff.
