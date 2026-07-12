# Building Deletion and Ground Resource Drops — Refined Feature Document

## Summary and goals

Players can instantly remove any completed building or cancel any building
blueprint from its right-click context panel. Deletion requires confirmation but
no worker, duration, or simulation tick.

Deletion converts recoverable materials into persistent, surface-local ground
stacks:

- Completed buildings drop their entire inventory plus 50% of each construction
  material, rounded down independently.
- Blueprints return every deposited material in full; construction labor is
  lost.
- Farm and Forester's Lodge deletion cascades to all linked Fields or Tree
  Plots.
- Colonists can consume loose stock directly or eventually haul it into
  storage.

## Player-facing behavior

- Completed buildings show `Delete Building`; blueprints show `Cancel
  Blueprint`.
- The action opens an exact confirmation dialog while the simulation continues
  running.
- The dialog refreshes live and shows:
  - Selected building name, kind, and completed/blueprint state.
  - Cascaded dependent counts grouped by kind and completed/blueprint state.
  - Aggregate quantities that will drop, grouped by resource kind.
- Confirming re-evaluates current authoritative state and performs the deletion
  atomically. Cancelling changes nothing.
- Successful deletion immediately closes the dialog and building panel, clears
  selection and hover state, removes building visuals, and displays the
  resulting ground stacks.
- If the target disappears, changes surface, or becomes invalid, the dialog
  closes without deleting anything. Other command failures remain visible
  through inline panel feedback.
- There is no global demolition tool or keyboard shortcut in this feature.

## Simulation and business rules

### Eligible targets and refunds

- Every `BuildingKind` is deletable, including Town Hall, houses, Fields, and
  Tree Plots.
- A completed building's refund uses its current
  `BuildingKind::definition().construction_cost()`.
- For each resource kind, refund `cost / 2` using integer floor division. A cost
  of one therefore refunds zero.
- A blueprint drops exactly its current `ConstructionProgress::deposited()`
  amounts. Labor has no value.
- Zero-quantity results create no stack.

"Building inventory" includes:

- Depot and Warehouse storage.
- Farm Crops.
- Forester's Lodge Wood.
- Both refinery input and output buffers.
- The original input unit consumed by an active refinery batch, regardless of
  production progress.

Partial refinery progress and plot growth have no salvage value. Deleting a
Field or Tree Plot discards seeds, crop/tree growth, and mature yields.

### Cascading deletion

- Deleting a Farm also deletes every existing blueprint or completed Field
  whose `FieldOwner` references that Farm.
- Deleting a Forester's Lodge does the same for linked Tree Plots.
- Each dependent uses its own completed-building or blueprint refund rule and
  drops resources across its own footprint.
- The root and all dependents are validated and deleted as one atomic operation.
- Deleting a Field or Tree Plot directly does not affect its owner.

### Work and entity cleanup

- Before despawning targets, immediately cancel all work, tasks, routes, and
  reservations that reference them, including while paused.
- Resources already withdrawn from a building are not duplicated in its drop:
  - Normal carried cargo stays with its colonist.
  - Loaded wheelbarrows follow the existing recovery behavior.
  - Unloaded invalid jobs are cancelled and their reservations released.
- Deleting a house immediately clears its residents' stale housing assignments.
  Normal housing maintenance reassigns them on the next simulation tick; while
  paused, they remain homeless.
- Navigation is refreshed for every deleted footprint immediately.

## Ground resource model

### Spatial distribution

For each deleted target:

1. Combine its inventory, active refinery input, and refund by resource kind.
2. Iterate nonzero kinds in the stable `ResourceKind::ALL` order.
3. Iterate the target footprint in existing row-major order.
4. Assign each complete resource-kind stack to the next cell, wrapping when
   kinds outnumber footprint cells.

A logical ground pile belongs to one cell and may contain several kind-specific
stacks after wrapping or merging.

### Lifetime, collision, and ownership

- Ground stacks are walkable and workers interact from their cell without
  gathering time or gathering XP.
- Any occupied ground-pile cell blocks new building, Field, Tree Plot, road, or
  road-upgrade placement until emptied.
- Stacks never decay and have no gameplay capacity.
- Drops at the same cell merge by resource kind without saturation or resource
  loss.
- Empty stacks are removed; an empty cell pile ceases to exist.
- Ground resources count immediately as usable owned resources in surface totals
  and remain isolated to their surface.
- Deletion never requires available storage capacity. If no destination or
  consumer is available, resources remain on the ground indefinitely.

### Logistics

Ground stock participates in the existing reservation and deterministic
source-selection model:

- Construction can collect required materials directly using normal
  carried-resource batches.
- Refinery work can source compatible ground inputs when its current source mode
  permits non-storage sources. A strict `Pull from Storage` setting remains
  strict.
- Hungry colonists may collect cooked `Food` directly into their Food Pouch;
  Crops and Wild Berries are not treated as Food.
- Leftovers are automatically hauled to reachable, completed, active storage
  that allows the resource and has reserved capacity.
- Ground-to-storage hauling uses the existing 25-unit wheelbarrow rule because
  its destination is storage.
- Full, filtered, inactive, removed, or unreachable storage leaves the source
  stack unchanged and eligible for later retry.
- Concurrent consumer and cleanup jobs reserve quantities so they cannot
  duplicate or overdraw a stack.
- Cleanup is the lowest-priority productive work: every other productive job
  outranks it, but cleanup outranks idle roaming.

### Rendering and inspection

- Reuse existing imported resource icons.
- Render each kind-specific stack in its assigned cell. When several kinds share
  a cell, arrange their icons within that tile.
- Quantities are not permanently printed on the map.
- Hovering a ground-pile cell shows `Items on Ground` and every resource kind and
  exact quantity in that cell.
- Ground piles are hover-only and do not open a separate selection panel.
- Preserve existing hover precedence, including NPCs taking precedence when
  standing on a pile.

## Architecture and interface contracts

- Durable deletion, cascade, refund, ground-stock, collision, reservation, and
  logistics rules belong in `game_engine`.
- Expose surface-scoped preview and execution operations using the existing
  `BuildingTarget` pattern.
  - Preview returns target state, dependent counts, and aggregate drop
    quantities.
  - Execution revalidates and returns the actual deletion/drop summary or a
    typed command error.
  - Invalid or duplicate commands are atomic and produce no resources.
- Represent ground stock as simulation-owned cell coordinates plus per-kind
  quantities, with deterministic queries for logistics, totals, rendering, and
  tooltips.
- Extend logistics stock endpoints with ground stock while preserving storage
  filters, refinery source modes, reservations, and stable tie-breaking.
- The Godot bridge remains thin: typed preview/execute calls, context-panel
  controls, confirmation presentation, pile rendering, and a typed ground-pile
  hover target.
- Use Rust-only Godot integration, typed exported references and signals, and
  `ResourceLoader` for existing icons.
- There is currently no save/load subsystem, so no migration is required.
  Ground piles are durable ECS state that a future save format must preserve.

## Non-goals

- Timed demolition, demolition labor, tools, damage, or cancellation after
  confirmation.
- Road deletion.
- Manually ordering, forbidding, moving, splitting, or prioritizing individual
  stacks.
- Resource decay, weather damage, or stack capacity gameplay.
- Salvaging plot growth or refinery production progress as finished output.
- New ground-item artwork.
- Adding save/load support.

## Acceptance and test expectations

### Simulation

- Completed buildings drop exactly all supported inventories plus correctly
  rounded refunds.
- Blueprints return deposited materials exactly, including partially and fully
  supplied sites.
- Active refinery deletion always returns the consumed input and never
  duplicates worker cargo.
- Farm/Lodge cascades cover mixed completed and blueprint dependents; plot growth
  produces no salvage.
- Distribution is deterministic for multi-cell and 1x1 footprints, including
  wrapping, same-cell merging, and empty-pile removal.
- Ground stock is walkable, blocks every construction type, counts as usable
  stock, and remains surface-local.
- Direct construction, refining, and cooked-Food use respect reservations and
  existing configuration.
- Cleanup respects filters, activity, capacity, reachability, wheelbarrow
  limits, deterministic selection, and last-before-idle priority.
- Deletion cleans related work immediately while preserving in-transit cargo.
- House assignments clear immediately and are repaired only on the next tick.
- Wrong-surface, stale, non-building, and duplicate commands fail without
  mutation or duplicate drops.
- Deletion while paused still removes entities and creates ground stock
  immediately.

### Bridge and UI

- Correct action label appears for completed buildings and blueprints.
- Confirmation updates live without pausing and presents exact aggregate
  resources and cascade counts.
- Cancel, stale-target, and surface-switch paths are non-mutating.
- Success closes selection UI and removes the building sprite without waiting
  for a simulation tick.
- Ground icons render deterministically, including several kinds on one cell.
- Hover tooltips show exact per-cell contents and disappear when the pile
  empties.
- Typed scene references load successfully in a headless Godot check.

### Validation

- `cargo test --manifest-path rust/Cargo.toml`
- `cargo build --manifest-path rust/Cargo.toml`
- Godot headless scene load with no registration, resource, or scene-wiring
  errors.

## Key decisions

- Blueprint cancellation returns full deposits.
- Completed refunds are per-kind half costs rounded down.
- Farm/Lodge deletion cascades and refunds linked plots.
- Active refinery production returns its original input.
- Ground resources are directly usable and automatically stored.
- Drops use one whole stack per resource kind, distributed row-major across each
  footprint.
- Stacks are walkable, construction-blocking, persistent, and unlimited.
- Cleanup is the lowest productive priority, immediately above idle roaming.
- Confirmation shows live counts and aggregate totals without pausing.
- House residents wait until the next simulation tick for reassignment.

## Open decisions

None.

## Repository evidence

- Building definitions, costs, blueprints, inventories, and completion:
  `rust/game_engine/src/buildings.rs`.
- Existing stock endpoints, construction/refinery logistics, and cancellation
  behavior: `rust/game_engine/src/logistics.rs`.
- Owned-resource totals and inventory primitives:
  `rust/game_engine/src/resources.rs`.
- Existing typed building context panel:
  `rust/godot_bridge/src/panel/building_info_panel.rs`.
- Map rendering, targeting, and resource-icon patterns:
  `rust/godot_bridge/src/world/game_world.rs`.
