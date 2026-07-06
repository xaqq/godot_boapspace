# Farming Feature Brief

## Summary

Add farming as a constructed food-production loop. The player builds a Farm,
assigns connected Field plots to that Farm, and Farmer-tagged NPCs seed and
harvest those fields over time. Each harvested field produces 1 Food into the
owning Farm's inventory, then returns to a seedable state.

The durable rules belong in `game_engine`. Godot should expose controls,
previews, rendering, selection details, and thin bridge calls only.

## Goals

- Add a new `Farm` building that owns nearby `Field` plots.
- Add `Field` as a real 1x1 building/blueprint type with construction cost and
  normal placement blocking.
- Let the player place many fields efficiently by selecting a Farm and dragging
  over tiles.
- Simulate per-field crop lifecycle: seedable, seeded/growing, grown, harvested,
  and seedable again.
- Use Farmer-tagged NPCs for farm work. Assignment is based on the tag, not on
  the `SkillKind::Farmer` skill value.
- Store harvested Food in the owning Farm's inventory.
- Keep all farming state scoped to the surface/world that contains the Farm.

## Non-Goals

- No crop variety, seasons, soil quality, irrigation, spoilage, fertilizer,
  weather, pests, or farm yield scaling.
- No diagonal field connectivity.
- No field placement restrictions based on terrain type.
- No use of GDScript.
- No cross-surface farming, storage, or worker assignment.
- No global colony storage system unless separately decided.

## Player-Facing Behavior

### Farm Building

- `Farm` is a new building kind.
- Footprint: 3x3.
- Construction cost: 20 Wood and 30 Stone.
- A Farm without linked Fields does not produce food.
- A Farm has a Food inventory for harvested crops.
- The Farm can be selected like other buildings and should expose:
  - construction progress while it is a blueprint,
  - footprint,
  - linked field count,
  - farm inventory,
  - enough state to understand why fields are inactive or active.

### Field Building

- `Field` is a real 1x1 building kind.
- Construction cost: 5 Wood and 1 Stone.
- Fields block later building placement, including other fields. A later
  building blueprint placed on top of an existing Field must be rejected.
- Fields may be placed on any terrain and over existing resource nodes or NPCs,
  consistent with the current normal-building behavior that does not reject
  resource-node or NPC overlap.
- Fields must be owned by one selected Farm when placed.
- Fields may be placed while the owning Farm is still a blueprint, but farming
  work remains inactive until both the Farm and the Field are constructed.

### Field Connectivity And Limits

- Field placement is valid only when the new Field is cardinally adjacent to:
  - any tile of the owning Farm's 3x3 footprint, or
  - an already linked Field owned by the same Farm.
- Diagonal adjacency does not count.
- Each Farm may own at most 200 Fields.
- The 200-Field limit counts all linked Field blueprints and constructed Fields
  for that Farm.
- Field placement is invalid when:
  - the target cell is out of bounds,
  - the target cell overlaps any existing building or building blueprint,
  - no owning Farm is selected,
  - the selected Farm cannot be resolved,
  - the field would not be cardinally connected to the selected Farm's field
    network,
  - the owning Farm already has 200 linked Fields.

### Field Placement UI

- Farm placement should use the normal building palette pattern.
- Field placement should be entered from a selected Farm rather than from a
  global standalone field button. This avoids ambiguous ownership when multiple
  Farms are nearby.
- Field placement should support drag placement over multiple cells.
- During drag placement, each tile is validated independently:
  - valid tiles are previewed as placeable,
  - invalid tiles are previewed as rejected,
  - duplicate tiles in the same drag are ignored,
  - already placed/queued fields are not duplicated.
- Releasing the drag places all valid Field blueprints in deterministic cell
  order. Invalid cells are skipped and should not cancel the whole drag.
- Right-click or the existing cancel action exits field placement mode.

## Simulation Rules

### Field Lifecycle

Field construction state is separate from crop state. A Field blueprint follows
the normal building construction flow. Once the Field is constructed, it has one
of these crop states:

- `Inactive`: the owning Farm is not constructed or no longer exists.
- `Seedable`: the Field is constructed, the owning Farm is constructed, and no
  crop is present.
- `Seeding`: a Farmer-tagged NPC is actively seeding the Field.
- `GrowingStep1`: crop growth has started; early visual stage.
- `GrowingStep2`: crop growth has progressed; later visual stage.
- `Grown`: crop is ready for harvest.

Seeding takes 1 in-game day of worker time. The simulation currently uses
1-minute fixed ticks, so this is 1,440 simulation ticks unless the time constants
change.

Seeding progress is stored on the Field, not on the NPC. If a Farmer is
interrupted, loses the Farmer tag, switches work, or disappears before seeding is
complete, the Field keeps its accumulated seeding progress and remains eligible
for another `SeedField` task instead of resetting to zero.

After seeding completes, that Field starts its own independent 1-year growth
timer. One year means 365 in-game days, matching the existing world-date age
logic. Growth does not wait for other Fields on the same Farm.

Recommended visual thresholds:

- `GrowingStep1`: from seeding completion through the first half of the growth
  duration.
- `GrowingStep2`: from halfway through the growth duration until harvest-ready.
- `Grown`: after the full 1-year duration has elapsed.

When a crop is harvested:

- Harvest work takes the existing one-hour resource gather duration,
  `RESOURCE_GATHER_TICKS_PER_UNIT`, currently 60 fixed ticks.
- 1 Food is added to the owning Farm's inventory.
- The Field returns to `Seedable`.
- The Field can generate another seeding task in a later tick.

### Farm Work Tasks

Farming should use explicit tasks, following the current construction task
pattern:

- `SeedField` task: represents the intention to seed one eligible Field.
- `HarvestField` task: represents the intention to harvest one grown Field.

Task maintenance should:

- create one seed task for each `Seedable` Field owned by a constructed Farm,
- create one harvest task for each `Grown` Field owned by a constructed Farm,
- avoid duplicate tasks across repeated ticks,
- remove stale tasks when the target Field, owning Farm, or required state no
  longer exists,
- run per surface, with no cross-surface references.

### Farmer Assignment

- Add a `Farmer` tag component.
- Add the `Farmer` tag to the NPC bundle so at least the initial NPC can perform
  farm work.
- Only NPCs with the `Farmer` tag can be assigned seeding or harvest tasks.
- Assignment does not depend on `SkillKind::Farmer` value or rank.
- Farming work should be mutually exclusive with existing active work states
  such as construction, gathering, food search, movement to another assigned
  task, and idle roaming.
- Existing food-refill behavior should continue to take priority over farming
  when an NPC needs food and food is available.
- Task selection should be deterministic. Prefer nearest actionable farming task
  by Manhattan distance, then lower `y`, lower `x`, and stable entity id as
  tie-breakers, matching existing AI conventions.

### Harvesting And Resources

- Grown Fields are not generic `ResourceNode` Food nodes.
- Hungry NPCs must not treat grown crops as ordinary food resource nodes.
- Generic resource gathering should continue to award `Forager` for wild Food.
- Crop harvest should be separate farming work and should store Food in the
  owning Farm's inventory instead of the NPC inventory.
- Completed seeding and completed harvest each award 1 `SkillKind::Farmer` XP,
  matching the existing one-XP-per-completed-gather convention.
- `SkillKind::Farmer` value does not affect farming eligibility, work speed,
  crop growth duration, or yield in this feature.

### Inventory

- Farm inventory stores harvested Food with a capacity of 200 Food.
- The Farm inventory should be shown in the building info panel.
- Food is deposited directly into the Farm inventory when harvest completes.
- If the Farm inventory cannot accept the Food, harvest completion must not
  destroy the crop.
- A full Farm inventory leaves the Field in `Grown`, does not consume the crop,
  and should show a blocked/full-storage state in Farm or Field details.
- `HarvestField` tasks should only be created or kept while the owning Farm has
  capacity for at least 1 Food. When capacity returns, the grown Field becomes
  harvestable again and can receive a new harvest task.
- Farm inventory is not a food source for hungry NPCs in v1. Existing food-refill
  AI continues to use Food `ResourceNode`s only.

### Terrain, Resource Nodes, And Overlap

- Field placement has no terrain restrictions beyond surface bounds.
- Field placement may overlap existing resource nodes because current building
  placement already permits resource-node overlap.
- Grown crops should not be rendered or queried through the existing
  `ResourceNode` layer, because that would make them eligible for generic
  gathering and food-search behavior.

### Pausing, Speed, And Time

- Farming systems run only when the simulation is playing.
- Paused ticks must not advance seeding progress or crop growth.
- Simulation speed multipliers advance farming progress by the corresponding
  number of fixed ticks, like hunger, movement, and construction.
- Farm and Field state should use the shared per-surface `WorldDateTime` or
  fixed-tick progress consistently with existing simulation systems.

## ECS And Data Implications

Likely durable simulation state belongs in `game_engine`:

- Extend `BuildingKind` with `Farm` and `Field`.
- Add Farm-specific inventory component.
- Add Field ownership and crop state components.
- Add Farmer tag component.
- Add farming tasks and AI state components for seeding/harvesting.
- Add systems for:
  - maintaining farming tasks,
  - assigning Farmer-tagged NPCs,
  - routing to target Fields,
  - advancing seeding work,
  - advancing crop growth,
  - completing harvest into Farm inventory,
  - cleaning stale farming tasks and AI state.

The existing construction systems should still build Farm and Field blueprints.
The construction completion system may need kind-specific component insertion,
similar to how Warehouse currently receives inventory on completion.

Farming data must not require Godot APIs. Godot-facing structs should be thin
queries or commands over simulation-owned state.

## Godot Bridge And UI Implications

- Add a Farm build button to the building palette.
- Add a Farm-selected action for entering Field placement mode.
- Extend the build/placement mode model so normal building placement and
  selected-Farm field placement can coexist without stringly method calls.
- Add bridge methods for:
  - starting Farm blueprint placement,
  - starting Field placement for the selected Farm,
  - validating field placement previews,
  - placing multiple Field blueprints for one Farm in one command,
  - querying Farm/Field details for panels and rendering.
- Render Farm and Field blueprints/constructed buildings with generated assets.
- Render Field crop states separately from generic resource nodes. This may be a
  dedicated field/crop tile layer or sprites keyed by Field entity.
- Extend selection/details so selected Fields show owning Farm, construction
  state, crop state, and progress where relevant.
- Extend task list rows to show `SeedField` and `HarvestField` tasks.
- Keep scene wiring typed with exported `OnEditor<Gd<T>>` references and Rust
  signal connections.

## Assets

Generate or add imported assets for:

- Farm building.
- Field blueprint/constructed base plot.
- Field crop states:
  - seedable/empty plot,
  - growing step 1,
  - growing step 2,
  - grown crop.

Assets under `res://` should be loaded through Godot's `ResourceLoader` as
resources, consistent with the current asset-loading rule.

## Edge Cases

- Placing a Field without a selected Farm is rejected.
- Placing a Field for a Farm on another surface is rejected.
- Placing a Field beyond 200 linked Fields is rejected.
- Placing a Field diagonally adjacent, but not cardinally connected, is rejected.
- Placing a normal building on top of a Field is rejected.
- Deleting or invalidating the owning Farm should make linked Fields inactive and
  remove any seed/harvest tasks for them. There is no current demolition feature,
  but the farming model should not assume dangling Farm entity references are
  valid forever.
- If a Farmer tag is removed from an NPC while it is working, the NPC should stop
  farming work and clear farming AI state.
- If a target Field disappears or changes state while an NPC is en route or
  working, the NPC should clear the farming AI state without awarding Food.
- If the owning Farm inventory is full or unavailable, harvest must not consume
  the grown crop.
- If simulation is paused, seeding and growth progress stay unchanged.
- Repeated ticks must not duplicate seed or harvest tasks.
- Switching surfaces in Godot should clear field placement mode and selected
  Farm/Field UI state for the old surface.

## Acceptance Criteria

- Farm definitions expose kind, 3x3 footprint, and 20 Wood / 30 Stone
  construction cost.
- Field definitions expose kind, 1x1 footprint, and 5 Wood / 1 Stone
  construction cost.
- Farm and Field blueprints use the normal construction task flow.
- Field placement requires a selected owning Farm and cardinal connectivity to
  that Farm's field network.
- Field placement rejects out-of-bounds cells, building/blueprint overlaps, and
  Farm field counts above 200.
- Field placement is scoped per surface.
- Constructed Fields remain inactive until their owning Farm is constructed.
- Each eligible constructed Field gets at most one seed task.
- A Farmer-tagged NPC can seed a Field after 1 in-game day of worker time.
- Partial seeding progress is stored on the Field and survives Farmer
  interruption.
- Each seeded Field grows independently for 365 in-game days.
- Growth state moves through `GrowingStep1`, `GrowingStep2`, and `Grown`.
- Each grown Field gets at most one harvest task.
- A Farmer-tagged NPC harvesting a grown Field after 60 fixed ticks adds exactly
  1 Food to the owning Farm inventory and returns the Field to `Seedable`.
- Completed seeding and completed harvest each award 1 `SkillKind::Farmer` XP.
- Grown crops are not visible to generic food search as `ResourceNode` Food.
- Farm inventory is not visible to NPC hunger refill or generic food search in
  v1.
- Paused simulation ticks do not advance seeding, growth, or harvest progress.
- Faster simulation speeds advance farming by the correct number of fixed ticks.
- Farm inventory appears in the building info UI.
- Task list UI includes seed and harvest tasks with useful target details.
- Drag placement can place multiple valid Field blueprints in one interaction and
  skips invalid drag cells without cancelling the valid placements.

## Tests And Validation Expectations

Simulation tests should cover:

- Farm and Field building definitions.
- Field placement success and each rejection reason.
- Cardinal adjacency and no diagonal connectivity.
- 200-field limit.
- Field placement and farming tasks scoped per surface.
- Construction completion inserts Farm inventory and Field crop state.
- Task maintenance creates, de-duplicates, and removes seed/harvest tasks.
- Farmer-tagged NPC assignment and non-Farmer exclusion.
- Seeding duration, Field-stored partial progress, pause behavior, and speed
  multiplier behavior.
- Independent growth timers for multiple Fields.
- Harvest takes 60 fixed ticks, adds exactly 1 Food to the Farm inventory, and
  cycles the Field back to `Seedable`.
- Completed seeding and completed harvest award Farmer XP without changing
  eligibility, speed, yield, or growth.
- Full/unavailable Farm inventory does not destroy a grown crop.
- Generic food search ignores grown crops.
- NPC hunger refill ignores Farm inventory in v1.

Bridge/UI tests should cover:

- Farm build button starts Farm placement.
- Selected-Farm field placement mode requires a valid Farm.
- Drag validation returns valid and invalid cells deterministically.
- Building/field render queries include blueprint and constructed states.
- Crop-state render queries do not reuse generic resource-node queries.
- Building info panel exposes Farm inventory and Field crop details.
- Task table rows include farming task types.

## Open Decisions

None. The previous open questions are resolved in this brief:

- Farm inventory capacity is 200 Food.
- Full Farm inventory leaves crops `Grown`, blocks harvest completion, and does
  not destroy Food.
- Farm inventory is visible storage only in v1; NPC hunger refill does not
  withdraw from it.
- Seeding progress is stored on the Field.
- Harvest work takes `RESOURCE_GATHER_TICKS_PER_UNIT`, currently 60 fixed ticks.
- Completed seeding and completed harvest each award 1 `SkillKind::Farmer` XP,
  but Farmer skill value does not affect farming behavior.
